//! `qlang serve app.q`: WebSockets, with a small client written here.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(name: &str, source: &str, extra: &[&str]) -> Server {
    let dir = std::env::temp_dir().join(format!("qlang-ws-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("app.q");
    std::fs::write(&file, source).unwrap();
    for _ in 0..10 {
        let port = TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("serve")
            .arg(&file)
            .args(["--port", &port.to_string()])
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stderr.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        if line.contains("serving") {
            return Server { child, port };
        }
        let _ = child.wait();
    }
    panic!("no free port");
}

struct Client {
    stream: TcpStream,
}

impl Client {
    /// Opens a WebSocket on `path`; the handshake answer is returned with the client.
    fn open(port: u16, path: &str, extra_headers: &str) -> (Client, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{extra_headers}\r\n"
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
        (
            Client { stream },
            String::from_utf8_lossy(&head).to_string(),
        )
    }

    fn send(&mut self, opcode: u8, payload: &[u8]) {
        let mask = [1u8, 2, 3, 4];
        let mut out = vec![0x80 | opcode];
        if payload.len() < 126 {
            out.push(0x80 | payload.len() as u8);
        } else {
            out.push(0x80 | 126);
            out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(&mask);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&out).unwrap();
    }

    fn text(&mut self, text: &str) {
        self.send(0x1, text.as_bytes());
    }

    /// The next frame the server sent: opcode and payload.
    fn frame(&mut self) -> Option<(u8, Vec<u8>)> {
        let mut head = [0u8; 2];
        self.stream.read_exact(&mut head).ok()?;
        let mut len = (head[1] & 0x7F) as usize;
        if len == 126 {
            let mut b = [0u8; 2];
            self.stream.read_exact(&mut b).ok()?;
            len = u16::from_be_bytes(b) as usize;
        }
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).ok()?;
        Some((head[0] & 0x0F, payload))
    }

    fn read_text(&mut self) -> String {
        loop {
            let (op, p) = self.frame().expect("a frame");
            if op == 0x1 {
                return String::from_utf8(p).unwrap();
            }
            assert_ne!(op, 0x8, "the server closed the connection");
        }
    }
}

const CHAT: &str = r#"
let members: array<WebSocket> = []

fun handle(req: Request) -> Response
  if req.path != "/chat" then
    return Response.text("plain page")
  end
  let name = req.query["name"] ?? "anon"
  Response.websocket(WebSocketHandler {
    tick_ms: 100,
    on_open: fun(c: WebSocket)
      members.push(c)
      c.data["name"] = Any.of(name)
      c.send("welcome {name}")
    end,
    on_message: fun(c: WebSocket, m: WsMessage)
      let text = m.text ?? ""
      if text == "quit" then
        c.close_with(4000, "quit")
        return
      end
      for other in members do
        if other.is_open() then
          other.send("{name}: {text}")
        end
      end
    end,
    on_tick: fun(c: WebSocket) end,
    on_close: fun(c: WebSocket)
      let keep: array<WebSocket> = []
      for o in members do
        if o.id != c.id then
          keep.push(o)
        end
      end
      members.clear()
      for o in keep do
        members.push(o)
      end
      for o in members do
        o.send("{name} left")
      end
    end,
  })
end
"#;

#[test]
fn the_handshake_accepts_and_messages_go_both_ways() {
    let server = start("hello", CHAT, &[]);
    let (mut a, head) = Client::open(server.port, "/chat?name=ada", "");
    assert!(head.starts_with("HTTP/1.1 101"), "{head}");
    assert!(head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    assert_eq!(a.read_text(), "welcome ada");
    a.text("hello");
    assert_eq!(a.read_text(), "ada: hello");
    // a ping is answered with a pong, a long message goes through
    a.send(0x9, b"x");
    assert_eq!(a.frame().unwrap(), (0xA, b"x".to_vec()));
    let long = "y".repeat(5000);
    a.text(&long);
    assert_eq!(a.read_text(), format!("ada: {long}"));
}

#[test]
fn one_worker_serves_several_connections_and_ordinary_requests() {
    let server = start("many", CHAT, &[]);
    let (mut a, _) = Client::open(server.port, "/chat?name=ada", "");
    assert_eq!(a.read_text(), "welcome ada");
    let (mut b, _) = Client::open(server.port, "/chat?name=bob", "");
    assert_eq!(b.read_text(), "welcome bob");
    a.text("hi all");
    assert_eq!(a.read_text(), "ada: hi all");
    assert_eq!(b.read_text(), "ada: hi all");
    // the only worker still answers HTTP while both stay open
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(b"GET /other HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut page = String::new();
    s.read_to_string(&mut page).unwrap();
    assert!(page.contains("plain page"), "{page}");
    // a close from the program ends one connection, the other hears about it
    a.text("quit");
    let mut closed = false;
    while let Some((op, p)) = a.frame() {
        if op == 0x8 {
            assert_eq!(&p[..2], &4000u16.to_be_bytes());
            closed = true;
            break;
        }
    }
    assert!(closed);
    assert_eq!(b.read_text(), "ada left");
}

#[test]
fn a_client_that_leaves_runs_on_close() {
    let server = start("leave", CHAT, &[]);
    let (mut a, _) = Client::open(server.port, "/chat?name=ada", "");
    a.read_text();
    let (mut b, _) = Client::open(server.port, "/chat?name=bob", "");
    b.read_text();
    drop(a);
    assert_eq!(b.read_text(), "ada left");
}

#[test]
fn a_plain_request_to_a_websocket_address_is_refused() {
    let server = start("plain", CHAT, &[]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(b"GET /chat HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 426"), "{answer}");
}

#[test]
fn the_ticks_come_and_a_bad_frame_closes_with_1002() {
    const TICKS: &str = r#"
fun handle(req: Request) -> Response
  Response.websocket(WebSocketHandler {
    tick_ms: 50,
    on_message: fun(c: WebSocket, m: WsMessage) end,
    on_tick: fun(c: WebSocket) c.send("tick") end,
  })
end
"#;
    let server = start("ticks", TICKS, &[]);
    let (mut a, _) = Client::open(server.port, "/", "");
    assert_eq!(a.read_text(), "tick");
    assert_eq!(a.read_text(), "tick");
    // an unmasked frame breaks the protocol
    a.stream.write_all(&[0x81, 0x02, b'h', b'i']).unwrap();
    let mut code = None;
    while let Some((op, p)) = a.frame() {
        if op == 0x8 {
            code = Some(u16::from_be_bytes([p[0], p[1]]));
            break;
        }
    }
    assert_eq!(code, Some(1002));
}

#[test]
fn an_error_in_a_callback_closes_that_connection_only() {
    const BAD: &str = r#"
fun handle(req: Request) -> Response
  Response.websocket(WebSocketHandler {
    on_message: fun(c: WebSocket, m: WsMessage)
      let xs: array<int> = []
      c.send("{xs[1]}")
    end,
  })
end
"#;
    let server = start("bad", BAD, &[]);
    let (mut a, _) = Client::open(server.port, "/", "");
    a.text("boom");
    let (op, p) = a.frame().unwrap();
    assert_eq!(op, 0x8);
    assert_eq!(&p[..2], &1011u16.to_be_bytes());
    // the worker is still there
    let (mut b, head) = Client::open(server.port, "/", "");
    assert!(head.starts_with("HTTP/1.1 101"));
    b.text("boom");
    assert_eq!(b.frame().unwrap().0, 0x8);
}
