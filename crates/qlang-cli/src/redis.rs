//! `Redis`: a client for the Redis protocol (RESP2), plain or over TLS (`rediss://`). The
//! prelude's `Redis` and `Subscription` are written in qlang over the four `extern fun` below;
//! this file is what answers them.

use qlang_core::host::{HostError, HostValue};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

pub const REDIS_EXTERNS: [&str; 4] = [
    "redis_connect",
    "redis_command",
    "redis_poll",
    "redis_close",
];

/// How long a connection waits for the server to answer an ordinary command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The biggest reply accepted (a bulk string or the whole buffer).
const MAX_REPLY: usize = 64 << 20;

/// What a server sends back.
#[derive(Debug, PartialEq, Clone)]
pub enum Reply {
    Simple(String),
    Error(String),
    Int(i64),
    /// `None`: the null bulk string.
    Bulk(Option<Vec<u8>>),
    /// `None`: the null array.
    Array(Option<Vec<Reply>>),
}

/// A command as the protocol sends it: an array of bulk strings.
pub fn encode_command(args: &[String]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for a in args {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn line_end(buf: &[u8], from: usize) -> Option<usize> {
    buf[from..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|i| from + i)
}

/// Reads one reply from the start of `buf`: the reply and how many bytes it took, `Ok(None)`
/// when the reply is not complete yet.
pub fn parse_reply(buf: &[u8]) -> Result<Option<(Reply, usize)>, String> {
    parse_at(buf, 0, 0)
}

fn parse_at(buf: &[u8], at: usize, depth: usize) -> Result<Option<(Reply, usize)>, String> {
    if depth > 32 {
        return Err("a reply is nested too deep".to_string());
    }
    let Some(&kind) = buf.get(at) else {
        return Ok(None);
    };
    let Some(end) = line_end(buf, at + 1) else {
        return Ok(None);
    };
    let line = std::str::from_utf8(&buf[at + 1..end]).map_err(|_| "a reply is not text")?;
    let next = end + 2;
    match kind {
        b'+' => Ok(Some((Reply::Simple(line.to_string()), next))),
        b'-' => Ok(Some((Reply::Error(line.to_string()), next))),
        b':' => {
            let n = line.parse().map_err(|_| "a number reply is not a number")?;
            Ok(Some((Reply::Int(n), next)))
        }
        b'$' => {
            let n: i64 = line.parse().map_err(|_| "a bulk length is not a number")?;
            if n < 0 {
                return Ok(Some((Reply::Bulk(None), next)));
            }
            let n = n as usize;
            if n > MAX_REPLY {
                return Err("a reply is too big".to_string());
            }
            if buf.len() < next + n + 2 {
                return Ok(None);
            }
            if &buf[next + n..next + n + 2] != b"\r\n" {
                return Err("a bulk string is not finished by a line break".to_string());
            }
            Ok(Some((
                Reply::Bulk(Some(buf[next..next + n].to_vec())),
                next + n + 2,
            )))
        }
        b'*' => {
            let n: i64 = line
                .parse()
                .map_err(|_| "an array length is not a number")?;
            if n < 0 {
                return Ok(Some((Reply::Array(None), next)));
            }
            let mut items = Vec::new();
            let mut pos = next;
            for _ in 0..n {
                match parse_at(buf, pos, depth + 1)? {
                    Some((item, after)) => {
                        items.push(item);
                        pos = after;
                    }
                    None => return Ok(None),
                }
            }
            Ok(Some((Reply::Array(Some(items)), pos)))
        }
        other => Err(format!("unknown reply type `{}`", other as char)),
    }
}

/// A reply as the program sees it (`Json`): numbers are floats, texts are texts, `nil` is null.
fn to_host(r: Reply) -> Result<HostValue, String> {
    Ok(match r {
        Reply::Simple(s) => HostValue::Str(s),
        Reply::Error(e) => return Err(e),
        Reply::Int(n) => HostValue::Float(n as f64),
        Reply::Bulk(None) | Reply::Array(None) => HostValue::Null,
        Reply::Bulk(Some(b)) => HostValue::Str(String::from_utf8_lossy(&b).into_owned()),
        Reply::Array(Some(items)) => HostValue::Array(
            items
                .into_iter()
                // an error inside an array (a transaction) stays an error text
                .map(|i| match i {
                    Reply::Error(e) => Ok(HostValue::Str(e)),
                    other => to_host(other),
                })
                .collect::<Result<_, _>>()?,
        ),
    })
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(s) => s,
            Stream::Tls(s) => &s.sock,
        }
    }
}

impl Read for Stream {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(b),
            Stream::Tls(s) => s.read(b),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(b),
            Stream::Tls(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

pub struct Conn {
    stream: Stream,
    buf: Vec<u8>,
}

/// The open connections of a host, by the number the program holds.
#[derive(Default)]
pub struct Pool {
    conns: HashMap<u64, Conn>,
    next: u64,
}

/// What a `redis://` URL says.
#[derive(Debug, PartialEq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub user: Option<String>,
    pub password: Option<String>,
    pub db: Option<u32>,
    /// `?tls=disable` was written on a server that is not on this machine.
    pub confirmed_plain: bool,
}

fn is_local(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.starts_with('/')
}

pub fn parse_url(url: &str) -> Result<Target, String> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("rediss://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("redis://") {
        (false, r)
    } else {
        return Err("a Redis URL starts with redis:// or rediss://".to_string());
    };
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let confirmed_plain = query.split('&').any(|p| p == "tls=disable");
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (credentials, hostport) = match authority.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, authority),
    };
    let (user, password) = match credentials {
        None => (None, None),
        Some(c) => match c.split_once(':') {
            Some((u, p)) => ((!u.is_empty()).then(|| u.to_string()), Some(p.to_string())),
            None => (None, Some(c.to_string())),
        },
    };
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, after) = v6
            .split_once(']')
            .ok_or("an IPv6 address needs a closing `]`")?;
        (h.to_string(), after.strip_prefix(':').unwrap_or(""))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (hostport.to_string(), ""),
        }
    };
    let port = if port.is_empty() {
        6379
    } else {
        port.parse().map_err(|_| "the port is not a number")?
    };
    let host = if host.is_empty() {
        "127.0.0.1".to_string()
    } else {
        host
    };
    let db = if path.is_empty() {
        None
    } else {
        Some(path.parse().map_err(|_| "the database is not a number")?)
    };
    Ok(Target {
        host,
        port,
        tls,
        user,
        password,
        db,
        confirmed_plain,
    })
}

fn io_err(e: std::io::Error) -> HostError {
    HostError::Other(format!("Redis: {e}"))
}

impl Conn {
    fn open(t: &Target) -> Result<Conn, HostError> {
        if !t.tls && !is_local(&t.host) && !t.confirmed_plain {
            return Err(HostError::Other(
                "this Redis server is not on this machine: use rediss:// (TLS), or add ?tls=disable to confirm that you want an unencrypted connection".to_string(),
            ));
        }
        let addr = (t.host.as_str(), t.port)
            .to_socket_addrs()
            .map_err(io_err)?
            .next()
            .ok_or_else(|| HostError::Other("Redis: the host has no address".to_string()))?;
        let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(io_err)?;
        let _ = tcp.set_nodelay(true);
        let stream = if t.tls {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let name = rustls::pki_types::ServerName::try_from(t.host.clone()).map_err(|_| {
                HostError::Other("Redis: the host name is not valid for TLS".into())
            })?;
            let conn = rustls::ClientConnection::new(Arc::new(config), name)
                .map_err(|e| HostError::Other(format!("Redis: {e}")))?;
            Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp)))
        } else {
            Stream::Plain(tcp)
        };
        let mut c = Conn {
            stream,
            buf: Vec::new(),
        };
        if let Some(pass) = &t.password {
            let mut args = vec!["AUTH".to_string()];
            args.extend(t.user.clone());
            args.push(pass.clone());
            c.command(&args, COMMAND_TIMEOUT)?;
        }
        if let Some(db) = t.db {
            c.command(&["SELECT".to_string(), db.to_string()], COMMAND_TIMEOUT)?;
        }
        Ok(c)
    }

    /// The next reply, waiting at most `wait` (`None`: forever). `Ok(None)` on a timeout.
    fn read_reply(&mut self, wait: Option<Duration>) -> Result<Option<Reply>, HostError> {
        let _ = self.stream.tcp().set_read_timeout(wait);
        loop {
            if let Some((r, used)) = parse_reply(&self.buf).map_err(HostError::Other)? {
                self.buf.drain(..used);
                return Ok(Some(r));
            }
            if self.buf.len() > MAX_REPLY {
                return Err(HostError::Other("Redis: a reply is too big".to_string()));
            }
            let mut chunk = [0u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(HostError::Other(
                        "Redis: the server closed the connection".to_string(),
                    ));
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(io_err(e)),
            }
        }
    }

    fn command(&mut self, args: &[String], wait: Duration) -> Result<Reply, HostError> {
        self.stream
            .write_all(&encode_command(args))
            .and_then(|()| self.stream.flush())
            .map_err(io_err)?;
        match self.read_reply(Some(wait))? {
            Some(r) => Ok(r),
            None => Err(HostError::Other(
                "Redis: the server did not answer in time".to_string(),
            )),
        }
    }
}

/// How long a command may take: a blocking pop waits as long as it asked for, plus a margin.
fn wait_for(args: &[String]) -> Option<Duration> {
    let name = args.first().map(|s| s.to_ascii_uppercase());
    match name.as_deref() {
        Some("BLPOP" | "BRPOP" | "BRPOPLPUSH" | "BLMOVE" | "BZPOPMIN" | "BZPOPMAX") => {
            let secs: f64 = args.last().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            if secs <= 0.0 {
                None
            } else {
                Some(Duration::from_secs_f64(secs + 5.0))
            }
        }
        _ => Some(COMMAND_TIMEOUT),
    }
}

impl Pool {
    pub fn connect(&mut self, url: &str) -> Result<u64, HostError> {
        let target = parse_url(url).map_err(|e| HostError::Other(format!("Redis: {e}")))?;
        let conn = Conn::open(&target)?;
        self.next += 1;
        self.conns.insert(self.next, conn);
        Ok(self.next)
    }

    fn get(&mut self, id: u64) -> Result<&mut Conn, HostError> {
        self.conns
            .get_mut(&id)
            .ok_or_else(|| HostError::Other("Redis: the connection is closed".to_string()))
    }

    pub fn command(&mut self, id: u64, args: &[String]) -> Result<HostValue, HostError> {
        let wait = wait_for(args);
        let c = self.get(id)?;
        c.stream
            .write_all(&encode_command(args))
            .and_then(|()| c.stream.flush())
            .map_err(io_err)?;
        match c.read_reply(wait)? {
            Some(r) => to_host(r).map_err(|e| HostError::Other(e)),
            None => Err(HostError::Other(
                "Redis: the server did not answer in time".to_string(),
            )),
        }
    }

    /// The next message of a subscription (`{"channel", "payload", "pattern"?}`), or `null` when
    /// none came in `timeout_ms`.
    pub fn poll(&mut self, id: u64, timeout_ms: u64) -> Result<HostValue, HostError> {
        let c = self.get(id)?;
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            // a zero timeout would mean "forever" to the socket: look at what is already there
            let wait = left.max(Duration::from_millis(1));
            let Some(reply) = c.read_reply(Some(wait))? else {
                return Ok(HostValue::Null);
            };
            let Reply::Array(Some(items)) = reply else {
                continue;
            };
            let text = |r: &Reply| match r {
                Reply::Bulk(Some(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                Reply::Simple(s) => Some(s.clone()),
                _ => None,
            };
            let kind = items.first().and_then(text).unwrap_or_default();
            let mut fields = Vec::new();
            match (kind.as_str(), items.len()) {
                ("message", 3) => {
                    fields.push(("channel", items.get(1).and_then(text)));
                    fields.push(("payload", items.get(2).and_then(text)));
                }
                ("pmessage", 4) => {
                    fields.push(("pattern", items.get(1).and_then(text)));
                    fields.push(("channel", items.get(2).and_then(text)));
                    fields.push(("payload", items.get(3).and_then(text)));
                }
                // confirmations of (un)subscribe: not a message
                _ => {
                    if std::time::Instant::now() >= deadline {
                        return Ok(HostValue::Null);
                    }
                    continue;
                }
            }
            return Ok(HostValue::Map(
                fields
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            HostValue::Str(k.to_string()),
                            v.map_or(HostValue::Null, HostValue::Str),
                        )
                    })
                    .collect(),
            ));
        }
    }

    pub fn close(&mut self, id: u64) {
        self.conns.remove(&id);
    }
}

/// Runs one of [`REDIS_EXTERNS`].
pub fn call(pool: &mut Pool, name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    let wrong = || HostError::Other(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        ("redis_connect", [HostValue::Str(url)]) => {
            pool.connect(url).map(|id| HostValue::Int(id as i64))
        }
        ("redis_command", [HostValue::Int(c), HostValue::Array(items)]) => {
            let words: Vec<String> = items
                .iter()
                .map(|v| match v {
                    HostValue::Str(s) => Ok(s.clone()),
                    _ => Err(wrong()),
                })
                .collect::<Result<_, _>>()?;
            if words.is_empty() {
                return Err(wrong());
            }
            pool.command(*c as u64, &words)
        }
        ("redis_poll", [HostValue::Int(c), HostValue::Int(ms)]) => {
            pool.poll(*c as u64, (*ms).clamp(0, 3_600_000) as u64)
        }
        ("redis_close", [HostValue::Int(c)]) => {
            pool.close(*c as u64);
            Ok(HostValue::Bool(true))
        }
        _ => Err(wrong()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn words(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn commands_are_arrays_of_bulk_strings() {
        assert_eq!(
            encode_command(&words(&["SET", "k", "héllo"])),
            b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$6\r\nh\xc3\xa9llo\r\n".to_vec()
        );
    }

    #[test]
    fn every_kind_of_reply_is_read() {
        let one = |b: &[u8]| parse_reply(b).unwrap().unwrap().0;
        assert_eq!(one(b"+OK\r\n"), Reply::Simple("OK".into()));
        assert_eq!(one(b"-ERR no\r\n"), Reply::Error("ERR no".into()));
        assert_eq!(one(b":42\r\n"), Reply::Int(42));
        assert_eq!(one(b"$3\r\nabc\r\n"), Reply::Bulk(Some(b"abc".to_vec())));
        assert_eq!(one(b"$-1\r\n"), Reply::Bulk(None));
        assert_eq!(one(b"*-1\r\n"), Reply::Array(None));
        assert_eq!(
            one(b"*2\r\n$1\r\na\r\n*1\r\n:1\r\n"),
            Reply::Array(Some(vec![
                Reply::Bulk(Some(b"a".to_vec())),
                Reply::Array(Some(vec![Reply::Int(1)]))
            ]))
        );
        // a bulk string may hold a line break
        assert_eq!(
            one(b"$4\r\na\r\nb\r\n"),
            Reply::Bulk(Some(b"a\r\nb".to_vec()))
        );
    }

    #[test]
    fn an_unfinished_reply_asks_for_more_and_a_bad_one_is_an_error() {
        assert!(parse_reply(b"").unwrap().is_none());
        assert!(parse_reply(b"$5\r\nab").unwrap().is_none());
        assert!(parse_reply(b"*2\r\n:1\r\n").unwrap().is_none());
        assert!(parse_reply(b"?x\r\n").is_err());
        assert!(parse_reply(b"$2\r\nabXX").is_err());
        // two replies in a row: the first is taken, with its length
        let (r, used) = parse_reply(b":1\r\n:2\r\n").unwrap().unwrap();
        assert_eq!((r, used), (Reply::Int(1), 4));
        // too deep
        let deep = "*1\r\n".repeat(40);
        assert!(parse_reply(deep.as_bytes()).is_err());
    }

    #[test]
    fn urls_are_read() {
        let t = parse_url("redis://:s3cret@cache.local:6380/2").unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.tls, t.db),
            ("cache.local", 6380, false, Some(2))
        );
        assert_eq!(t.password.as_deref(), Some("s3cret"));
        assert_eq!(t.user, None);
        let t = parse_url("rediss://app:pw@h/").unwrap();
        assert!(t.tls && t.user.as_deref() == Some("app") && t.port == 6379);
        let t = parse_url("redis://localhost").unwrap();
        assert_eq!((t.host.as_str(), t.port), ("localhost", 6379));
        assert!(parse_url("redis://[::1]:7000").unwrap().host == "::1");
        assert!(parse_url("http://x").is_err());
        assert!(parse_url("redis://h:abc").is_err());
    }

    #[test]
    fn a_remote_server_in_clear_needs_a_confirmation() {
        let err = Pool::default()
            .connect("redis://example.invalid")
            .unwrap_err();
        assert!(format!("{err:?}").contains("not on this machine"));
    }

    /// A one-connection server that answers each command with the next canned reply.
    fn fake(script: Vec<(&'static str, Vec<u8>)>) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = Vec::new();
            for (_, reply) in script {
                // read one command
                loop {
                    if let Some((Reply::Array(Some(items)), used)) = parse_reply(&buf).unwrap() {
                        let line: Vec<String> = items
                            .iter()
                            .map(|i| match i {
                                Reply::Bulk(Some(b)) => String::from_utf8_lossy(b).into_owned(),
                                _ => String::new(),
                            })
                            .collect();
                        seen.push(line.join(" "));
                        buf.drain(..used);
                        break;
                    }
                    let mut chunk = [0u8; 1024];
                    let n = s.read(&mut chunk).unwrap();
                    if n == 0 {
                        return seen;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                s.write_all(&reply).unwrap();
            }
            // stay there until the client leaves
            let mut chunk = [0u8; 64];
            while matches!(s.read(&mut chunk), Ok(n) if n > 0) {}
            seen
        });
        (port, handle)
    }

    #[test]
    fn commands_and_a_subscription_go_through_a_real_socket() {
        let (port, server) = fake(vec![
            ("", b"+OK\r\n".to_vec()),
            ("", b"$3\r\nbar\r\n".to_vec()),
            ("", b"-WRONGTYPE bad\r\n".to_vec()),
            (
                "",
                b"*3\r\n$9\r\nsubscribe\r\n$4\r\nnews\r\n:1\r\n*3\r\n$7\r\nmessage\r\n$4\r\nnews\r\n$2\r\nhi\r\n"
                    .to_vec(),
            ),
        ]);
        let mut pool = Pool::default();
        let id = pool.connect(&format!("redis://127.0.0.1:{port}")).unwrap();
        assert_eq!(
            pool.command(id, &words(&["SET", "foo", "bar"])).unwrap(),
            HostValue::Str("OK".into())
        );
        assert_eq!(
            pool.command(id, &words(&["GET", "foo"])).unwrap(),
            HostValue::Str("bar".into())
        );
        let err = pool
            .command(id, &words(&["LPUSH", "foo", "x"]))
            .unwrap_err();
        assert!(format!("{err:?}").contains("WRONGTYPE"));
        // the confirmation of the subscription, then a message
        let first = pool.command(id, &words(&["SUBSCRIBE", "news"])).unwrap();
        assert!(matches!(first, HostValue::Array(_)));
        match pool.poll(id, 500).unwrap() {
            HostValue::Map(m) => {
                assert!(m.contains(&(
                    HostValue::Str("payload".into()),
                    HostValue::Str("hi".into())
                )));
                assert!(m.contains(&(
                    HostValue::Str("channel".into()),
                    HostValue::Str("news".into())
                )));
            }
            other => panic!("{other:?}"),
        }
        // nothing else comes
        assert_eq!(pool.poll(id, 30).unwrap(), HostValue::Null);
        pool.close(id);
        assert!(pool.command(id, &words(&["PING"])).is_err());
        let seen = server.join().unwrap();
        assert_eq!(seen[0], "SET foo bar");
        assert_eq!(seen[3], "SUBSCRIBE news");
    }
}
