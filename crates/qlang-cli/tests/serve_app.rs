//! `qlang serve app.q`: a qlang program answers real HTTP requests.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

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

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn app_file(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-serve-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.q");
    std::fs::write(&path, source).unwrap();
    path
}

fn start(name: &str, source: &str, extra: &[&str]) -> Server {
    let file = app_file(name, source);
    // another test may take the port between `free_port` and the server's `bind`: try again
    for _ in 0..10 {
        let port = free_port();
        let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("serve")
            .arg(&file)
            .args(["--port", &port.to_string()])
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // the server says it is listening on its standard error
        let mut line = String::new();
        BufReader::new(child.stderr.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        if line.contains("serving") {
            return Server { child, port };
        }
        assert!(line.contains("cannot listen"), "unexpected start: {line}");
        let _ = child.wait();
    }
    panic!("no free port");
}

/// Sends raw text, returns everything the server answers.
fn exchange(server: &Server, request: &str) -> String {
    exchange_port(server.port, request)
}

fn exchange_port(port: u16, request: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

const APP: &str = r#"
let hits = 0

fun handle(req: Request) -> Response
  hits += 1
  if req.url == "/json" then
    return Response { status: 200, headers: { "Content-Type": "application/json" }, body: "\{\"hits\": {hits}\}" }
  end
  if req.url == "/boom" then
    panic("secret detail")
  end
  if req.url == "/split" then
    return Response { status: 200, headers: { "X-Evil": "a\r\nSet-Cookie: x=1" }, body: "ok" }
  end
  if req.method == "POST" then
    let agent = req.headers.get("user-agent", "?")
    let body = req.body
    let text = if body == none then "" else body end
    return Response { status: 201, body: "got {text} from {agent}" }
  end
  Response { status: 200, body: "hello {req.url} #{hits}" }
end
"#;

#[test]
fn answers_get_and_post_and_keeps_its_state() {
    let server = start("basic", APP, &[]);
    let a = exchange(&server, "GET /one?x=1 HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(a.starts_with("HTTP/1.1 200 OK\r\n"), "{a}");
    assert!(a.ends_with("hello /one?x=1 #1"), "{a}");
    assert!(
        a.contains("Content-Type: text/plain; charset=utf-8\r\n"),
        "{a}"
    );
    assert!(a.contains("Content-Length: 17\r\n"), "{a}");
    let b = exchange(&server, "GET /two HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(b.ends_with("hello /two #2"), "{b}");
    let p = exchange(
        &server,
        "POST /form HTTP/1.1\r\nHost: t\r\nUser-Agent: curl\r\nContent-Length: 5\r\n\r\nhello",
    );
    assert!(p.starts_with("HTTP/1.1 201 Created\r\n"), "{p}");
    assert!(p.ends_with("got hello from curl"), "{p}");
}

#[test]
fn the_program_chooses_its_headers() {
    let server = start("headers", APP, &[]);
    let r = exchange(&server, "GET /json HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.contains("Content-Type: application/json\r\n"), "{r}");
    assert!(!r.contains("text/plain"), "{r}");
    assert!(r.ends_with("{\"hits\": 1}"), "{r}");
    // a header with a line break cannot inject another one
    let r = exchange(&server, "GET /split HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(!r.contains("Set-Cookie"), "{r}");
}

#[test]
fn an_error_is_a_500_without_details_and_the_server_goes_on() {
    let server = start("errors", APP, &[]);
    let r = exchange(&server, "GET /boom HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(
        r.starts_with("HTTP/1.1 500 Internal Server Error\r\n"),
        "{r}"
    );
    assert!(!r.contains("secret detail"), "{r}");
    let r = exchange(&server, "GET /after HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 200 OK\r\n"), "{r}");
}

#[test]
fn bad_requests_are_refused() {
    let server = start("bad", APP, &[]);
    assert!(exchange(&server, "garbage\r\n\r\n").starts_with("HTTP/1.1 400"));
    let huge = format!(
        "POST /x HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n",
        5 << 20
    );
    assert!(exchange(&server, &huge).starts_with("HTTP/1.1 413"));
    assert!(
        exchange(
            &server,
            "POST /x HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n"
        )
        .starts_with("HTTP/1.1 501")
    );
}

#[test]
fn head_has_no_body() {
    let server = start("head", APP, &[]);
    let r = exchange(&server, "HEAD /x HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 200 OK\r\n"), "{r}");
    assert!(r.ends_with("\r\n\r\n"), "{r}");
}

#[test]
fn the_step_limit_applies_to_each_request() {
    let src = "fun handle(req: Request) -> Response\n  if req.url == \"/loop\" then\n    while true do\n    end\n  end\n  Response { status: 200, body: \"fine\" }\nend";
    let server = start("limit", src, &["--max-steps", "100000"]);
    assert!(exchange(&server, "GET /loop HTTP/1.1\r\nHost: t\r\n\r\n").starts_with("HTTP/1.1 500"));
    assert!(exchange(&server, "GET /ok HTTP/1.1\r\nHost: t\r\n\r\n").ends_with("fine"));
}

#[test]
fn a_program_without_handle_does_not_start() {
    let file = app_file("nohandle", "print(1)");
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("serve")
        .arg(&file)
        .args(["--port", &free_port().to_string()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("handle"));
}

#[test]
fn the_json_server_still_works() {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["serve", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let server = Server { child, port };
    let body = r#"{"files":{"main.q":"print(6 * 7)"}}"#;
    let r = exchange(
        &server,
        &format!(
            "POST /run HTTP/1.1\r\nHost: localhost:{}\r\nContent-Length: {}\r\n\r\n{body}",
            server.port,
            body.len()
        ),
    );
    assert!(r.contains("\"output\":\"42\\n\""), "{r}");
    assert!(
        exchange(&server, "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .contains("\"status\":\"ok\"")
    );
    // a web page cannot use it, directly or through a name made to point here
    for extra in [
        "Origin: https://evil.example\r\n",
        "Sec-Fetch-Site: cross-site\r\n",
    ] {
        let r = exchange(
            &server,
            &format!(
                "POST /run HTTP/1.1\r\nHost: localhost\r\n{extra}Content-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(r.starts_with("HTTP/1.1 403 "), "{extra}: {r}");
    }
    let r = exchange(
        &server,
        &format!(
            "POST /run HTTP/1.1\r\nHost: evil.example\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(r.starts_with("HTTP/1.1 403 "), "{r}");
}

#[test]
fn the_example_serves() {
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/app.q"))
            .unwrap();
    let server = start("example", Box::leak(source.into_boxed_str()), &[]);
    let r = exchange(&server, "GET /hello?name=Ada HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.ends_with("Bonjour Ada !\n"), "{r}");
    assert!(
        exchange(&server, "GET /visits HTTP/1.1\r\nHost: t\r\n\r\n").contains("application/json")
    );
    let r = exchange(
        &server,
        "POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 3\r\n\r\nabc",
    );
    assert!(r.ends_with("abc"), "{r}");
    assert!(exchange(&server, "GET /x HTTP/1.1\r\nHost: t\r\n\r\n").starts_with("HTTP/1.1 404"));
}

const ECHO_APP: &str = r#"
fun handle(req: Request) -> Response
  let x = req.query.get("x", "-")
  let y = req.query.get("y", "-")
  let agent = req.header("USER-AGENT")
  let a = if agent == none then "-" else agent end
  Response.text("path={req.path} x={x} y={y} agent={a} url={req.url}")
end
"#;

#[test]
fn the_path_and_the_query_are_decoded() {
    let server = start("query", ECHO_APP, &[]);
    let r = exchange(
        &server,
        "GET /a%20b/c?x=1&x=2&y=%C3%A9+z HTTP/1.1\r\nHost: t\r\nUser-Agent: curl\r\n\r\n",
    );
    assert!(
        r.ends_with("path=/a b/c x=2 y=é z agent=curl url=/a%20b/c?x=1&x=2&y=%C3%A9+z"),
        "{r}"
    );
    let r = exchange(&server, "GET / HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.ends_with("path=/ x=- y=- agent=- url=/"), "{r}");
}

#[test]
fn static_files_are_served_before_the_program_and_cannot_escape() {
    let root = std::env::temp_dir().join(format!("qlang-static-test-{}", std::process::id()));
    let public = root.join("public");
    std::fs::create_dir_all(public.join("sub")).unwrap();
    std::fs::write(public.join("hello.txt"), "static hello").unwrap();
    std::fs::write(public.join("index.html"), "<h1>home</h1>").unwrap();
    std::fs::write(public.join("sub/style.css"), "body{}").unwrap();
    std::fs::write(root.join("secret.txt"), "TOP SECRET").unwrap();
    std::fs::write(public.join(".env"), "TOKEN=hidden").unwrap();
    std::fs::create_dir_all(public.join(".git")).unwrap();
    std::fs::write(public.join(".git/config"), "HIDDEN CONFIG").unwrap();
    let server = start("static", ECHO_APP, &["--static", public.to_str().unwrap()]);

    let r = exchange(&server, "GET /hello.txt HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(
        r.starts_with("HTTP/1.1 200 OK\r\n")
            && r.contains("Content-Type: text/plain; charset=utf-8\r\n")
            && r.ends_with("static hello"),
        "{r}"
    );
    let r = exchange(&server, "GET / HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(
        r.contains("Content-Type: text/html; charset=utf-8\r\n") && r.ends_with("<h1>home</h1>"),
        "{r}"
    );
    let r = exchange(&server, "GET /sub/style.css HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(
        r.contains("Content-Type: text/css; charset=utf-8\r\n"),
        "{r}"
    );
    // what is not a file goes to the program
    let r = exchange(&server, "GET /nothing?x=7 HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.contains("path=/nothing x=7"), "{r}");
    // only GET and HEAD read files
    let r = exchange(
        &server,
        "POST /hello.txt HTTP/1.1\r\nHost: t\r\nContent-Length: 0\r\n\r\n",
    );
    assert!(r.contains("path=/hello.txt"), "{r}");
    // hidden files and folders are not files for the web
    for target in ["/.env", "/.git/config", "/%2eenv"] {
        let r = exchange(
            &server,
            &format!("GET {target} HTTP/1.1\r\nHost: t\r\n\r\n"),
        );
        assert!(
            !r.contains("hidden") && !r.contains("HIDDEN"),
            "{target}: {r}"
        );
    }
    // nothing outside the folder, whatever the spelling
    for target in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/sub/../../secret.txt",
        "/%2E%2E%2Fsecret.txt",
    ] {
        let r = exchange(
            &server,
            &format!("GET {target} HTTP/1.1\r\nHost: t\r\n\r\n"),
        );
        assert!(!r.contains("TOP SECRET"), "{target}: {r}");
    }
}

/// A program whose `/slow` request spins until a file appears: no timing involved.
fn busy_app(flag: &std::path::Path) -> String {
    let flag = flag.to_string_lossy().replace('\\', "/");
    r#"
fun handle(req: Request) -> Response
  if req.url == "/slow" then
    while fs.read("FLAG").is_err() do
    end
  end
  Response.text("done {req.url}")
end
"#
    .replace("FLAG", &flag)
}

fn flag_file(name: &str) -> PathBuf {
    let flag = std::env::temp_dir().join(format!("qlang-flag-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&flag);
    flag
}

fn send_get(port: u16, target: &str) -> TcpStream {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(format!("GET {target} HTTP/1.1\r\nHost: t\r\n\r\n").as_bytes())
        .unwrap();
    s
}

fn finish(mut s: TcpStream) -> String {
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn workers_answer_while_another_one_is_busy() {
    let flag = flag_file("two");
    let source: &'static str = Box::leak(busy_app(&flag).into_boxed_str());
    let server = start(
        "workers2",
        source,
        &["--workers", "2", "--max-steps", "2000000000"],
    );
    let slow = send_get(server.port, "/slow");
    std::thread::sleep(std::time::Duration::from_millis(300));
    // the other worker answers while /slow is still spinning
    let fast = exchange(&server, "GET /fast HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(fast.ends_with("done /fast"), "{fast}");
    std::fs::write(&flag, "go").unwrap();
    assert!(finish(slow).ends_with("done /slow"));
}

#[test]
fn with_one_worker_a_busy_request_makes_the_others_wait() {
    let flag = flag_file("one");
    let source: &'static str = Box::leak(busy_app(&flag).into_boxed_str());
    let server = start("workers1", source, &["--max-steps", "2000000000"]);
    let slow = send_get(server.port, "/slow");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let fast = send_get(server.port, "/fast");
    fast.set_read_timeout(Some(std::time::Duration::from_millis(700)))
        .unwrap();
    let mut early = String::new();
    let mut fast = fast;
    assert!(
        fast.read_to_string(&mut early).is_err(),
        "answered while the only worker was busy: {early}"
    );
    std::fs::write(&flag, "go").unwrap();
    assert!(finish(slow).ends_with("done /slow"));
    fast.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    assert!((early + &finish(fast)).ends_with("done /fast"));
}

#[cfg(unix)]
#[test]
fn it_stops_cleanly_when_asked() {
    let mut server = start("stop", APP, &[]);
    assert!(exchange(&server, "GET /x HTTP/1.1\r\nHost: t\r\n\r\n").starts_with("HTTP/1.1 200"));
    let pid = server.child.id().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(s) = server.child.try_wait().unwrap() {
            break s;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the server did not stop"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
}

#[test]
fn the_api_example_routes_requests() {
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/api.q"))
            .unwrap();
    let server = start("api", Box::leak(source.into_boxed_str()), &[]);
    let r = exchange(&server, "GET /items HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.contains("application/json") && r.ends_with("[]"), "{r}");
    assert!(
        exchange(
            &server,
            "POST /items/ordinateur HTTP/1.1\r\nHost: t\r\nContent-Length: 0\r\n\r\n"
        )
        .starts_with("HTTP/1.1 201")
    );
    assert!(
        exchange(
            &server,
            "POST /items/c%C3%A2ble HTTP/1.1\r\nHost: t\r\nContent-Length: 0\r\n\r\n"
        )
        .starts_with("HTTP/1.1 201")
    );
    let r = exchange(&server, "GET /items HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.ends_with("[\"ordinateur\",\"câble\"]"), "{r}");
    assert!(
        exchange(&server, "GET /items/ordinateur HTTP/1.1\r\nHost: t\r\n\r\n")
            .ends_with("ordinateur")
    );
    assert!(
        exchange(&server, "GET /items/absent HTTP/1.1\r\nHost: t\r\n\r\n")
            .starts_with("HTTP/1.1 404")
    );
    let r = exchange(
        &server,
        "DELETE /items/ordinateur HTTP/1.1\r\nHost: t\r\n\r\n",
    );
    assert!(
        r.starts_with("HTTP/1.1 405") && r.contains("Allow: GET, POST"),
        "{r}"
    );
}

#[test]
fn a_status_a_client_cannot_understand_is_a_500() {
    let server = start(
        "badstatus",
        "fun handle(req: Request) -> Response\n  Response { status: 99999, body: \"x\" }\nend\n",
        &[],
    );
    let r = exchange(&server, "GET / HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 500 "), "{r}");
}

#[test]
fn a_client_sending_too_slowly_is_cut_off() {
    use std::io::Write;
    let server = start("slow", ECHO_APP, &[]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(40)))
        .unwrap();
    // the head never ends: one byte every second must not keep the connection for ever
    let started = std::time::Instant::now();
    let mut closed = false;
    for _ in 0..40 {
        if s.write_all(b"X").is_err() {
            closed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        // the server answers 408 and closes: the next write fails, or the read says so
        s.set_nonblocking(true).unwrap();
        let mut b = [0u8; 64];
        if matches!(std::io::Read::read(&mut s, &mut b), Ok(n) if n > 0) {
            closed = true;
            break;
        }
        s.set_nonblocking(false).unwrap();
    }
    assert!(closed, "the connection was still open after 40 s");
    assert!(started.elapsed() < std::time::Duration::from_secs(35));
}

#[test]
fn watch_reloads_the_program_and_keeps_the_old_one_on_a_mistake() {
    let v = |text: &str| {
        format!("fun handle(req: Request) -> Response\n  Response.text(\"{text}\")\nend\n")
    };
    let server = start("watch", &v("one"), &["--watch"]);
    // the texts differ in length: a file system with a coarse clock may keep the same time
    let path = app_file("watch", &v("one"));
    let get = |server: &Server| exchange(server, "GET / HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(get(&server).ends_with("one"));
    let until = |server: &Server, want: &str| {
        for _ in 0..40 {
            if get(server).ends_with(want) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    };
    std::fs::write(&path, v("second")).unwrap();
    assert!(until(&server, "second"), "the program was not reloaded");
    // a mistake: the previous version keeps answering
    std::fs::write(&path, "fun handle(req: Request) -> Response\n  nope\nend\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let kept = get(&server);
    assert!(kept.ends_with("second"), "{kept}");
    std::fs::write(&path, v("third one")).unwrap();
    assert!(
        until(&server, "third one"),
        "the fixed program was not reloaded"
    );
}

#[test]
fn the_routes_example_finds_its_handlers_by_reflection() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/routes.q"
    ))
    .unwrap();
    let server = start("routes", Box::leak(source.into_boxed_str()), &[]);
    let get = |path: &str| exchange(&server, &format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n"));
    assert!(get("/").ends_with("Accueil"));
    assert!(get("/hello?name=Ada").ends_with("Bonjour Ada !"));
    assert!(get("/nothing").starts_with("HTTP/1.1 404"));
    // the route is a POST one
    assert!(get("/echo").starts_with("HTTP/1.1 404"));
    let r = exchange(
        &server,
        "POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\n\r\nping",
    );
    assert!(r.ends_with("ping"), "{r}");
}
