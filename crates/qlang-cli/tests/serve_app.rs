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
    TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

fn app_file(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-serve-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("app.q");
    std::fs::write(&path, source).unwrap();
    path
}

fn start(name: &str, source: &str, extra: &[&str]) -> Server {
    let port = free_port();
    let file = app_file(name, source);
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
    BufReader::new(child.stderr.take().unwrap()).read_line(&mut line).unwrap();
    assert!(line.contains("serving"), "unexpected start: {line}");
    Server { child, port }
}

/// Sends raw text, returns everything the server answers.
fn exchange(server: &Server, request: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
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
    assert!(a.contains("Content-Type: text/plain; charset=utf-8\r\n"), "{a}");
    assert!(a.contains("Content-Length: 17\r\n"), "{a}");
    let b = exchange(&server, "GET /two HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(b.ends_with("hello /two #2"), "{b}");
    let p = exchange(&server, "POST /form HTTP/1.1\r\nHost: t\r\nUser-Agent: curl\r\nContent-Length: 5\r\n\r\nhello");
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
    assert!(r.starts_with("HTTP/1.1 500 Internal Server Error\r\n"), "{r}");
    assert!(!r.contains("secret detail"), "{r}");
    let r = exchange(&server, "GET /after HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 200 OK\r\n"), "{r}");
}

#[test]
fn bad_requests_are_refused() {
    let server = start("bad", APP, &[]);
    assert!(exchange(&server, "garbage\r\n\r\n").starts_with("HTTP/1.1 400"));
    let huge = format!("POST /x HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n", 5 << 20);
    assert!(exchange(&server, &huge).starts_with("HTTP/1.1 413"));
    assert!(exchange(&server, "POST /x HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n").starts_with("HTTP/1.1 501"));
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
    let out = Command::new(env!("CARGO_BIN_EXE_qlang")).arg("serve").arg(&file).args(["--port", &free_port().to_string()]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("handle"));
}

#[test]
fn the_json_server_still_works() {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang")).args(["serve", "--port", &port.to_string()]).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap()).read_line(&mut line).unwrap();
    let server = Server { child, port };
    let body = r#"{"files":{"main.q":"print(6 * 7)"}}"#;
    let r = exchange(&server, &format!("POST /run HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
    assert!(r.contains("\"output\":\"42\\n\""), "{r}");
    assert!(exchange(&server, "GET /health HTTP/1.1\r\nHost: t\r\n\r\n").contains("\"status\":\"ok\""));
}

#[test]
fn the_example_serves() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/app.q")).unwrap();
    let server = start("example", Box::leak(source.into_boxed_str()), &[]);
    let r = exchange(&server, "GET /hello HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.ends_with("Bonjour !\n"), "{r}");
    let r = exchange(&server, "POST /echo HTTP/1.1\r\nHost: t\r\nContent-Length: 3\r\n\r\nabc");
    assert!(r.ends_with("abc"), "{r}");
    assert!(exchange(&server, "GET /x HTTP/1.1\r\nHost: t\r\n\r\n").starts_with("HTTP/1.1 404"));
}
