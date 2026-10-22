//! A tiny HTTP server. Two modes:
//!
//! - `qlang serve`: the JSON protocol (`POST /run`, `GET /health`);
//! - `qlang serve app.q`: a qlang program answers the requests with its `handle` function.
//!
//! It is deliberately minimal (no TLS, no keep-alive). Put it behind a
//! reverse proxy and bind it to localhost unless you know what you are doing.

use crate::json::{self, Ceiling};
use qlang_core::check::compile;
use qlang_core::host::Limits;
use qlang_core::interp::{AppRequest, AppResponse};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const MAX_BODY: usize = 1 << 20;
const MAX_CONNECTIONS: usize = 16;

pub fn serve(args: &[&str]) -> ExitCode {
    let mut port: u16 = 8080;
    let mut bind = "127.0.0.1".to_string();
    let mut app = AppOptions {
        workers: 1,
        ..AppOptions::default()
    };
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--" {
            app.script_args = args[i + 1..].iter().map(|s| s.to_string()).collect();
            break;
        }
        match args[i] {
            "--max-steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => app.max_steps = Some(n),
                    None => {
                        eprintln!("--max-steps needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--watch" => app.watch = true,
            "--trust-externs" => app.trust_externs = true,
            "--allow-process" => app.allow_process = true,
            "--extern" => {
                i += 1;
                match args.get(i) {
                    Some(command) => app.extern_commands.push(command.to_string()),
                    None => {
                        eprintln!("--extern needs a command");
                        return ExitCode::from(64);
                    }
                }
            }
            "--seed" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => app.seed = Some(n),
                    None => {
                        eprintln!("--seed needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--root" => {
                i += 1;
                match args.get(i).map(|d| Path::new(d).canonicalize()) {
                    Some(Ok(dir)) if dir.is_dir() => app.root = Some(dir),
                    _ => {
                        eprintln!("--root needs an existing directory");
                        return ExitCode::from(64);
                    }
                }
            }
            "--static" => {
                i += 1;
                match args.get(i).map(|d| Path::new(d).canonicalize()) {
                    Some(Ok(dir)) if dir.is_dir() => app.static_dir = Some(dir),
                    _ => {
                        eprintln!("--static needs an existing directory");
                        return ExitCode::from(64);
                    }
                }
            }
            "--workers" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse::<usize>().ok()) {
                    Some(n) if (1..=64).contains(&n) => app.workers = n,
                    _ => {
                        eprintln!("--workers needs a number from 1 to 64");
                        return ExitCode::from(64);
                    }
                }
            }
            "--port" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(p) => port = p,
                    None => {
                        eprintln!("--port needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--bind" => {
                i += 1;
                match args.get(i) {
                    Some(b) => bind = b.to_string(),
                    None => {
                        eprintln!("--bind needs an address");
                        return ExitCode::from(64);
                    }
                }
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option `{other}`");
                return ExitCode::from(64);
            }
            file if app.file.is_none() => app.file = Some(file.to_string()),
            other => {
                eprintln!("unexpected argument `{other}`");
                return ExitCode::from(64);
            }
        }
        i += 1;
    }
    if let Some(file) = &app.file {
        match crate::providers::specs_for(Path::new(file), &app.extern_commands).and_then(|specs| {
            crate::providers::ensure_trusted(&specs, app.trust_externs).map(|()| specs)
        }) {
            Ok(specs) => app.providers = specs,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
        if let Some(dir) = crate::project_dir(Path::new(file)) {
            match crate::packages::Packages::load(&dir) {
                Ok(p) => app.packages = p,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(1);
                }
            }
        }
        return serve_app(&bind, port, app);
    }
    if app.max_steps.is_some()
        || app.seed.is_some()
        || app.root.is_some()
        || app.static_dir.is_some()
        || app.workers != 1
        || !app.script_args.is_empty()
        || !app.extern_commands.is_empty()
        || app.trust_externs
        || app.allow_process
    {
        eprintln!(
            "--max-steps, --seed, --root, --static, --workers and `--` only apply to `qlang serve <file.q>`"
        );
        return ExitCode::from(64);
    }
    let listener = match TcpListener::bind((bind.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on {bind}:{port}: {e}");
            return ExitCode::from(74);
        }
    };
    eprintln!("qlang serving on http://{bind}:{port} (POST /run)");
    let loopback = is_loopback(&bind);
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            let _ = respond(stream, 503, "{\"error\":\"server busy\"}");
            continue;
        }
        let slot = Slot::take(&active);
        let spawned = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                // the slot is freed when the thread ends, whatever happens (including a panic)
                let _slot = slot;
                let reply = stream.try_clone();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_connection(stream, loopback)
                }));
                if outcome.is_err()
                    && let Ok(s) = reply
                {
                    let _ = respond(s, 500, "{\"error\":\"internal error\"}");
                }
            });
        if spawned.is_err() {
            eprintln!("cannot start a worker thread");
        }
    }
    ExitCode::SUCCESS
}

/// One of the limited connection slots. Freed on drop, so a panicking worker cannot leak it.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(counter: &Arc<AtomicUsize>) -> Slot {
        counter.fetch_add(1, Ordering::SeqCst);
        Slot(counter.clone())
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        408 => "Request Timeout",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Status",
    }
}

fn respond(stream: TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    write_response(
        stream,
        status,
        &[("Content-Type".to_string(), "application/json".to_string())],
        body.as_bytes(),
        false,
    )
}

/// Writes a response. Header names and values with line breaks are dropped (a program could
/// otherwise split the response), and the ones this server decides itself are ignored.
fn write_response(
    mut stream: TcpStream,
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (name, value) in headers {
        let fixed = ["content-length", "connection", "transfer-encoding"]
            .contains(&name.to_ascii_lowercase().as_str());
        let valid = !name.is_empty()
            && name.bytes().all(|c| c.is_ascii_graphic() && c != b':')
            && !value.contains(['\r', '\n']);
        if valid && !fixed {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    stream.write_all(head.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}

/// An HTTP request as read from a connection. Header names are in lower case.
struct Parsed {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// How long a client has to send its whole request.
const READ_DEADLINE: Duration = Duration::from_secs(20);

/// Arms the read timeout with what is left before `deadline`; `false` when there is nothing left.
fn wait_for_data(stream: &TcpStream, deadline: Instant) -> bool {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return false;
    }
    let _ = stream.set_read_timeout(Some(left.min(Duration::from_secs(10))));
    true
}

/// Reads one request. On failure, the status and message to answer with (`None`: the client
/// went away, nothing to answer).
fn read_request(
    stream: &mut TcpStream,
    max_body: usize,
) -> Result<Parsed, Option<(u16, &'static str)>> {
    // the whole request must arrive in time, not just every piece of it: a client sending a
    // byte every few seconds would otherwise hold one of the few connections for ever
    let deadline = Instant::now() + READ_DEADLINE;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(p) = find(&buf, b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 64 * 1024 {
            return Err(Some((400, "headers too large")));
        }
        if !wait_for_data(stream, deadline) {
            return Err(Some((408, "request timeout")));
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(None),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    );
    if method.is_empty() || target.is_empty() {
        return Err(Some((400, "malformed request")));
    }
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    if header("transfer-encoding").is_some() {
        return Err(Some((501, "chunked requests are not supported")));
    }
    let content_length = header("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > max_body {
        return Err(Some((413, "request too large")));
    }
    let mut body: Vec<u8> = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        if !wait_for_data(stream, deadline) {
            return Err(Some((408, "request timeout")));
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(content_length);
    Ok(Parsed {
        method,
        target,
        headers,
        body,
    })
}

/// Whether the server listens on this machine only.
fn is_loopback(bind: &str) -> bool {
    bind == "localhost"
        || bind
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Whether a `Host` header names this machine (`localhost`, `127.0.0.1`, `[::1]`, with a port).
fn is_local_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.rsplit_once(':').map_or(host, |(h, _)| h),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// A web page is not a client of this server: a request that comes from a browser page
/// (`Origin` is always sent by browsers on a cross-site POST) is refused, and so is one
/// addressed to another name than this machine's when the server is only on this machine
/// (a page of another site whose name was made to point here: DNS rebinding).
fn refuse_browser(request: &Parsed, loopback: bool) -> Option<&'static str> {
    let header = |name: &str| {
        request
            .headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    if header("origin").is_some() || header("sec-fetch-site").is_some_and(|v| v != "none") {
        return Some("requests from a web page are refused");
    }
    if loopback && header("host").is_some_and(|h| !is_local_host(h)) {
        return Some("this server only answers to localhost");
    }
    None
}

fn handle_connection(mut stream: TcpStream, loopback: bool) {
    let request = match read_request(&mut stream, MAX_BODY) {
        Ok(r) => r,
        Err(Some((status, message))) => {
            let _ = respond(stream, status, &format!("{{\"error\":\"{message}\"}}"));
            return;
        }
        Err(None) => return,
    };
    if let Some(why) = refuse_browser(&request, loopback) {
        let _ = respond(stream, 403, &format!("{{\"error\":\"{why}\"}}"));
        return;
    }
    match (request.method.as_str(), request.target.as_str()) {
        ("GET", "/health") => {
            let _ = respond(stream, 200, "{\"status\":\"ok\"}");
        }
        ("POST", "/run") => {
            let text = String::from_utf8_lossy(&request.body).to_string();
            let out = json::handle(&text, &Ceiling::default());
            let _ = respond(stream, 200, &out);
        }
        (_, "/run") | (_, "/health") => {
            let _ = respond(stream, 405, "{\"error\":\"method not allowed\"}");
        }
        _ => {
            let _ = respond(stream, 404, "{\"error\":\"not found\"}");
        }
    }
}

// ------------------------------------------------------------------ a program as the server

#[derive(Default)]
struct AppOptions {
    file: Option<String>,
    max_steps: Option<u64>,
    seed: Option<u64>,
    root: Option<PathBuf>,
    /// Files served as they are, before asking the program (`--static DIR`).
    static_dir: Option<PathBuf>,
    /// Interpreters answering requests, each with its own copy of the program.
    workers: usize,
    script_args: Vec<String>,
    /// `--extern` commands, and the providers they and `qlang.toml` declare.
    extern_commands: Vec<String>,
    trust_externs: bool,
    allow_process: bool,
    /// Reload the program when one of its files changes (`--watch`).
    watch: bool,
    providers: Vec<crate::providers::Spec>,
    packages: crate::packages::Packages,
}

/// A request waiting for an interpreter, and where to send the answer.
struct Job {
    request: AppRequest,
    reply: mpsc::Sender<Result<AppResponse, String>>,
}

enum Msg {
    Job(Job),
    /// The server is stopping: a worker that reads this ends.
    Stop,
}

/// How long a connection waits for the program before answering 504.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
/// Set when the server was asked to stop.
static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// How long a stopping server lets the requests in progress finish.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Writes to the server's log (standard error). Unlike `eprint!` it never panics when the log
/// cannot be written (a closed terminal or pipe must not stop requests being answered).
fn log(text: &str) {
    let _ = std::io::stderr().write_all(text.as_bytes());
}

/// `qlang serve app.q`: every worker runs the program's top-level code once, then calls its
/// `handle(req: Request) -> Response` for the requests it takes from the queue. A worker has
/// its own copy of the program, so its variables are its own (use a database or files for what
/// must be shared). Connections are read in parallel, so a slow client blocks nobody.
fn serve_app(bind: &str, port: u16, opts: AppOptions) -> ExitCode {
    let file = opts.file.clone().unwrap_or_default();
    let source = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            return ExitCode::from(66);
        }
    };
    if let (Some(root), Ok(full)) = (&opts.root, Path::new(&file).canonicalize())
        && !full.starts_with(root)
    {
        eprintln!("{file} is outside of --root {}", root.display());
        return ExitCode::from(64);
    }
    let (jobs, queue) = mpsc::channel::<Msg>();
    let queue = Arc::new(Mutex::new(queue));
    let active = Arc::new(AtomicUsize::new(0));
    let opts = Arc::new(opts);
    let bind = bind.to_string();

    // the other workers start once the first one is ready (so that mistakes are reported once)
    let ready = {
        let (jobs, queue, active, opts, file, source, bind) = (
            jobs.clone(),
            queue.clone(),
            active.clone(),
            opts.clone(),
            file.clone(),
            source.clone(),
            bind.clone(),
        );
        move || -> Result<(), ExitCode> {
            let listener = match TcpListener::bind((bind.as_str(), port)) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("cannot listen on {bind}:{port}: {e}");
                    return Err(ExitCode::from(74));
                }
            };
            let workers = opts.workers;
            let more = if workers == 1 {
                String::new()
            } else {
                format!(" ({workers} workers)")
            };
            log(&format!(
                "qlang serving {file} on http://{bind}:{port}{more}\n"
            ));
            for index in 1..workers {
                let (queue, opts, file, source) =
                    (queue.clone(), opts.clone(), file.clone(), source.clone());
                let started = std::thread::Builder::new()
                    .stack_size(256 << 20)
                    .spawn(move || {
                        if let Err(code) = worker(index, &file, &source, &opts, &queue, || Ok(())) {
                            log(&format!("worker {index} could not start\n"));
                            std::process::exit(if code == ExitCode::SUCCESS { 0 } else { 70 });
                        }
                    });
                if started.is_err() {
                    log("cannot start a worker thread\n");
                }
            }
            // stop on Ctrl-C or `kill`: the requests already queued are still answered
            let stop = jobs.clone();
            let _ = ctrlc::set_handler(move || {
                STOPPING.store(true, Ordering::SeqCst);
                for _ in 0..workers {
                    let _ = stop.send(Msg::Stop);
                }
            });
            let opts = opts.clone();
            let acceptor = std::thread::Builder::new().spawn(move || {
                for stream in listener.incoming().flatten() {
                    if STOPPING.load(Ordering::SeqCst) {
                        let _ = write_response(stream, 503, &[], b"server stopping", false);
                        continue;
                    }
                    if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                        let _ = write_response(stream, 503, &[], b"server busy", false);
                        continue;
                    }
                    let slot = Slot::take(&active);
                    let (jobs, opts) = (jobs.clone(), opts.clone());
                    let _ = std::thread::Builder::new().spawn(move || {
                        let _slot = slot;
                        serve_connection(stream, &jobs, opts.static_dir.as_deref());
                    });
                }
            });
            if acceptor.is_err() {
                eprintln!("cannot start the listener thread");
                return Err(ExitCode::from(70));
            }
            Ok(())
        }
    };
    // this thread is the first worker (it has the big stack, see `main`)
    let outcome = worker(0, &file, &source, &opts, &queue, ready);
    // wait a little for the answers still being written
    let deadline = Instant::now() + STOP_GRACE;
    while active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

/// Bumped by the watcher each time a file of the program changes (`--watch`).
static RELOAD: AtomicU64 = AtomicU64::new(0);
/// The last version of `RELOAD` whose outcome was announced (once, not once per worker).
static ANNOUNCED: AtomicU64 = AtomicU64::new(0);
/// The files of the running program, for the watcher.
static WATCHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A cheap fingerprint of the watched files: their sizes and modification times.
fn fingerprint() -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    let files = WATCHED.lock().map(|f| f.clone()).unwrap_or_default();
    files
        .into_iter()
        .map(|f| match std::fs::metadata(&f) {
            Ok(m) => (f, m.len(), m.modified().ok()),
            Err(_) => (f, 0, None),
        })
        .collect()
}

/// Watches the files of the program; a change that stays put for a moment bumps `RELOAD`.
fn watch_files() {
    let _ = std::thread::Builder::new().spawn(|| {
        let mut last = fingerprint();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let now = fingerprint();
            if now == last {
                continue;
            }
            // editors write a file in several steps: wait for it to settle
            std::thread::sleep(Duration::from_millis(100));
            last = fingerprint();
            RELOAD.fetch_add(1, Ordering::SeqCst);
        }
    });
}

/// Compiles the program as it is now, without running it. The new source when it is sound;
/// otherwise the mistakes are announced (once) and the running version stays.
fn reload_check(file: &str, opts: &AppOptions, version: u64) -> Option<String> {
    let announce = || ANNOUNCED.swap(version, Ordering::SeqCst) != version;
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            if announce() {
                log(&format!(
                    "reload: cannot read {file}: {e}; keeping the previous version\n"
                ));
            }
            return None;
        }
    };
    let mut host = crate::TerminalHost {
        root: opts.root.clone(),
        rng: 0,
        db: Default::default(),
        script_args: opts.script_args.clone(),
        providers: Default::default(),
        packages: opts.packages.clone(),
        assume_externs: true,
        allow_process: opts.allow_process,
    };
    match compile(file, &source, &mut host) {
        Ok(_) => {
            if announce() {
                log("reloaded\n");
            }
            Some(source)
        }
        Err(e) => {
            if announce() {
                log(&e.render());
                let n = e.diagnostics.len();
                log(&format!(
                    "reload: {n} error{}; keeping the previous version\n",
                    if n == 1 { "" } else { "s" }
                ));
            }
            None
        }
    }
}

/// One interpreter: compiles the program, runs it, then answers requests until told to stop.
/// `ready` runs once the program has started (the first worker uses it to open the port).
/// With `--watch` it starts over from the new source when a file has changed.
fn worker(
    index: usize,
    file: &str,
    source: &str,
    opts: &AppOptions,
    queue: &Arc<Mutex<mpsc::Receiver<Msg>>>,
    ready: impl FnOnce() -> Result<(), ExitCode>,
) -> Result<(), ExitCode> {
    let mut source = source.to_string();
    let mut ready = Some(ready);
    let mut seen = RELOAD.load(Ordering::SeqCst);
    // the request that was taken from the queue just before a reload
    let mut pending: Option<Job> = None;
    loop {
        let seed = opts
            .seed
            .map(|s| s.wrapping_add(index as u64))
            .unwrap_or_else(crate::system_entropy);
        let providers = match crate::providers::Providers::start(
            &opts.providers,
            &qlang_core::host::SQL_EXTERNS,
        ) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return Err(ExitCode::from(1));
            }
        };
        let mut host = crate::TerminalHost {
            root: opts.root.clone(),
            rng: seed,
            db: Default::default(),
            script_args: opts.script_args.clone(),
            providers,
            packages: opts.packages.clone(),
            assume_externs: false,
            allow_process: opts.allow_process,
        };
        let program = match compile(file, &source, &mut host) {
            Ok(p) => p,
            Err(e) => {
                eprint!("{}", e.render());
                let n = e.diagnostics.len();
                eprintln!("\n{n} error{} found", if n == 1 { "" } else { "s" });
                return Err(ExitCode::from(1));
            }
        };
        if index == 0 {
            for w in &program.warnings {
                eprint!("{}", w.render(&program.sources));
            }
            if opts.watch {
                let mut files: Vec<String> = vec![file.to_string()];
                files.extend(program.sources.files.iter().map(|f| f.name.clone()));
                files.dedup();
                if let Ok(mut w) = WATCHED.lock() {
                    *w = files;
                }
            }
        }
        let mut limits = Limits::default();
        if let Some(n) = opts.max_steps {
            limits.max_steps = n;
        }
        let mut app = match program.start_app(&mut host, limits) {
            Ok(a) => a,
            Err(d) => {
                eprint!("{}", d.render(&program.sources));
                return Err(ExitCode::from(2));
            }
        };
        if let Some(r) = ready.take() {
            r()?;
            if opts.watch {
                watch_files();
            }
        }
        loop {
            // only one worker waits on the queue at a time, the others wait for their turn
            let msg = match pending.take() {
                Some(job) => Ok(Msg::Job(job)),
                None => match queue.lock() {
                    Ok(q) => q.recv(),
                    Err(_) => return Ok(()),
                },
            };
            match msg {
                Ok(Msg::Job(job)) => {
                    let now = RELOAD.load(Ordering::SeqCst);
                    if opts.watch && now != seen {
                        seen = now;
                        if let Some(new) = reload_check(file, opts, now) {
                            source = new;
                            pending = Some(job);
                            break;
                        }
                    }
                    let answer = app
                        .handle(job.request)
                        .map_err(|d| d.render(&program.sources));
                    let _ = job.reply.send(answer);
                }
                Ok(Msg::Stop) | Err(_) => return Ok(()),
            }
        }
    }
}

/// `%41` to `A`. In a query, `+` is a space.
fn percent_decode(text: &str, plus_is_space: bool) -> String {
    let b = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len()
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit() =>
            {
                out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The decoded path and query parameters of a request target (`/a%20b?x=1&y=2`).
fn split_target(target: &str) -> (String, Vec<(String, String)>) {
    let target = target.split('#').next().unwrap_or("");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let params = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k, true), percent_decode(v, true))
        })
        .collect();
    (percent_decode(path, false), params)
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// The file `path` (a decoded request path) names inside `dir`, if there is one. Paths that
/// try to leave `dir` (`..`, links pointing outside) never match.
fn static_file(dir: &Path, path: &str) -> Option<PathBuf> {
    if path.contains('\0') || path.split('/').any(|part| part == "..") {
        return None;
    }
    // hidden files (`.env`, `.git/config`...) are never served, whatever the folder holds
    if path
        .split('/')
        .any(|part| part.starts_with('.') && part != ".well-known")
    {
        return None;
    }
    let mut full = dir.join(path.trim_start_matches('/'));
    if full.is_dir() {
        full = full.join("index.html");
    }
    let full = full.canonicalize().ok()?;
    (full.starts_with(dir) && full.is_file()).then_some(full)
}

/// Biggest file served with `--static` (bigger ones would be read whole into memory).
const MAX_STATIC: u64 = 50 << 20;

fn serve_connection(mut stream: TcpStream, jobs: &mpsc::Sender<Msg>, static_dir: Option<&Path>) {
    let started = Instant::now();
    let request = match read_request(&mut stream, MAX_BODY) {
        Ok(r) => r,
        Err(Some((status, message))) => {
            let _ = write_response(stream, status, &[], message.as_bytes(), false);
            return;
        }
        Err(None) => return,
    };
    let head_only = request.method == "HEAD";
    let line = format!("{} {}", request.method, request.target);
    let (path, query) = split_target(&request.target);

    // files first, like a web server in front of the program
    if let (Some(dir), true) = (
        static_dir,
        matches!(request.method.as_str(), "GET" | "HEAD"),
    ) && let Some(file) = static_file(dir, &path)
        && std::fs::metadata(&file)
            .map(|m| m.len() <= MAX_STATIC)
            .unwrap_or(false)
        && let Ok(bytes) = std::fs::read(&file)
    {
        let headers = [
            ("Content-Type".to_string(), content_type(&file).to_string()),
            ("X-Content-Type-Options".to_string(), "nosniff".to_string()),
        ];
        let _ = write_response(stream, 200, &headers, &bytes, head_only);
        log(&format!(
            "{line} -> 200 static ({} ms)\n",
            started.elapsed().as_millis()
        ));
        return;
    }

    let body = if request.body.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&request.body).to_string())
    };
    let (reply, answer) = mpsc::channel();
    let app_request = AppRequest {
        method: request.method,
        url: request.target,
        headers: request.headers,
        body,
        path,
        query,
    };
    if STOPPING.load(Ordering::SeqCst)
        || jobs
            .send(Msg::Job(Job {
                request: app_request,
                reply,
            }))
            .is_err()
    {
        let _ = write_response(stream, 503, &[], b"server stopping", false);
        return;
    }
    let text_plain = [(
        "Content-Type".to_string(),
        "text/plain; charset=utf-8".to_string(),
    )];
    let status = match answer.recv_timeout(ANSWER_TIMEOUT) {
        Ok(Ok(r)) => {
            let mut headers = r.headers;
            if !headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            {
                headers.extend(text_plain);
            }
            // a status a client could not understand is the program's mistake
            let status = u16::try_from(r.status)
                .ok()
                .filter(|s| (100..=599).contains(s))
                .unwrap_or(500);
            let _ = write_response(stream, status, &headers, r.body.as_bytes(), head_only);
            status
        }
        Ok(Err(diagnostic)) => {
            // the details stay in the server's log: they are not for the client
            log(&diagnostic);
            let _ = write_response(
                stream,
                500,
                &text_plain,
                b"Internal Server Error",
                head_only,
            );
            500
        }
        Err(_) => {
            let _ = write_response(
                stream,
                504,
                &[],
                b"the program took too long to answer",
                false,
            );
            504
        }
    };
    log(&format!(
        "{line} -> {status} ({} ms)\n",
        started.elapsed().as_millis()
    ));
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_freed_even_when_the_worker_panics() {
        let counter = Arc::new(AtomicUsize::new(0));
        let slot = Slot::take(&counter);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        let worker = std::thread::spawn(move || {
            let _slot = slot;
            panic!("worker failed");
        });
        assert!(worker.join().is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 0, "the slot leaked");
    }

    #[test]
    fn slots_count_up_and_down() {
        let counter = Arc::new(AtomicUsize::new(0));
        let a = Slot::take(&counter);
        let b = Slot::take(&counter);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        drop(a);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        drop(b);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}
