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
    // the server keeps a connection open for the next request: this one is the last
    let request = if request.to_ascii_lowercase().contains("connection:") {
        request.to_string()
    } else {
        request.replacen("\r\n", "\r\nConnection: close\r\n", 1)
    };
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
            "POST /x HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: gzip\r\n\r\n"
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
    // a busy machine (CI runs many tests at once) may take a while to notice a change
    let until = |server: &Server, want: &str| {
        for _ in 0..150 {
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

const WEB: &str = r#"
fun handle(req: Request) -> Response
  if req.path == "/cookies" then
    let who = req.cookies.get("who", "nobody")
    let res = Response.text("hi {who}")
    res.with_cookie(Cookie { name: "a", value: "x y", max_age: 60 })
    res.with_cookie(Cookie { name: "b", value: "2", http_only: false, secure: true, same_site: "Strict" })
    return res
  end
  if req.path == "/query" then
    let all = req.query_all.get("t", [])
    let joined = all.join(",")
    let last = req.query.get("t", "?")
    return Response.text("{all.len()} {joined} last={last}")
  end
  if req.path == "/form" then
    let form = req.form()
    if form.is_err() then
      return Response.text(form.unwrap_err().message).with_status(400)
    end
    let f = form.unwrap()
    let name = f.get("name", "?")
    let note = f.get("note", "?")
    return Response.text("{name}|{note}")
  end
  if req.path == "/dup" then
    let tags = req.header_all("X-Tag")
    return Response.text(tags.join("+"))
  end
  if req.path == "/bad-cookie" then
    return Response.text("x").with_cookie(Cookie { name: "bad name", value: "1" })
  end
  if req.path == "/boom" then
    panic("secret detail")
  end
  Response.not_found()
end
"#;

#[test]
fn cookies_come_in_and_go_out_one_header_each() {
    let server = start("cookies", WEB, &[]);
    let r = exchange(
        &server,
        "GET /cookies HTTP/1.1\r\nHost: t\r\nCookie: who=Ada%20L; other=1\r\n\r\n",
    );
    assert!(r.ends_with("hi Ada L"), "{r}");
    assert!(
        r.contains("Set-Cookie: a=x%20y; Path=/; Max-Age=60; HttpOnly; SameSite=Lax\r\n"),
        "{r}"
    );
    assert!(
        r.contains("Set-Cookie: b=2; Path=/; Secure; SameSite=Strict\r\n"),
        "{r}"
    );
    // a cookie that cannot be sent is the program's mistake
    let r = exchange(&server, "GET /bad-cookie HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 500"), "{r}");
}

#[test]
fn repeated_query_parameters_and_headers_are_kept() {
    let server = start("repeats", WEB, &[]);
    let r = exchange(
        &server,
        "GET /query?t=1&t=2&u=3 HTTP/1.1\r\nHost: t\r\n\r\n",
    );
    assert!(r.ends_with("2 1,2 last=2"), "{r}");
    let r = exchange(
        &server,
        "GET /dup HTTP/1.1\r\nHost: t\r\nX-Tag: a\r\nx-tag: b\r\n\r\n",
    );
    assert!(r.ends_with("a+b"), "{r}");
}

#[test]
fn a_form_body_is_decoded() {
    let server = start("form", WEB, &[]);
    let body = "name=Ada+L&note=a%26b%3Dc";
    let r = exchange(
        &server,
        &format!(
            "POST /form HTTP/1.1\r\nHost: t\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(r.ends_with("Ada L|a&b=c"), "{r}");
    let r = exchange(
        &server,
        "POST /form HTTP/1.1\r\nHost: t\r\nContent-Length: 1\r\n\r\nx",
    );
    assert!(r.starts_with("HTTP/1.1 400"), "{r}");
}

#[test]
fn dev_mode_shows_the_error_and_production_does_not() {
    let dev = start("dev", WEB, &["--dev"]);
    let r = exchange(&dev, "GET /boom HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 500"), "{r}");
    assert!(r.contains("secret detail"), "{r}");
    let prod = start("prod", WEB, &[]);
    let r = exchange(&prod, "GET /boom HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(!r.contains("secret detail"), "{r}");
}

// ----------------------------------------------------------------- the server itself

/// Reads one response with a `Content-Length` from a connection that stays open.
fn read_one(s: &mut TcpStream) -> (String, Vec<u8>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        buf.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buf).to_string();
    let len: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).unwrap();
    (head, body)
}

const SERVER: &str = r#"
let n = 0

fun handle(req: Request) -> Response
  n += 1
  if req.path == "/bytes" then
    let raw = req.raw
    if raw == none then
      return Response.text("none")
    end
    let tail = bytes.from_array([0, 255])
    if tail == none then
      return Response.text("unreachable")
    end
    return Response.bytes(raw.concat(tail), "application/octet-stream")
  end
  if req.path == "/big" then
    let line = "hello hello hello hello hello hello hello hello\n"
    return Response.text(line.repeat(200))
  end
  if req.path == "/upload" then
    let parts = req.multipart()
    if parts.is_err() then
      return Response.text(parts.unwrap_err().message).with_status(400)
    end
    let out = ""
    for p in parts.unwrap() do
      out += "{p.name}:{p.data.len()};"
    end
    return Response.text(out)
  end
  if req.path == "/events" then
    let i = 0
    return Response.sse(fun() -> SseEvent?
      i += 1
      if i > 3 then
        return none
      end
      SseEvent { data: "tick {i}", event: "t" }
    end)
  end
  if req.path == "/stream" then
    let i = 0
    return Response.stream("text/plain", fun() -> string?
      i += 1
      if i > 2 then none else "part{i}\n" end
    end)
  end
  Response.text("n={n} body={req.body}")
end
"#;

#[test]
fn a_connection_stays_open_for_several_requests() {
    let server = start("keepalive", SERVER, &[]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    for expected in ["n=1", "n=2", "n=3"] {
        s.write_all(b"GET /x HTTP/1.1\r\nHost: t\r\n\r\n").unwrap();
        let (head, body) = read_one(&mut s);
        assert!(head.contains("Connection: keep-alive"), "{head}");
        assert!(String::from_utf8_lossy(&body).starts_with(expected));
    }
    // two requests sent at once are both answered, in order
    s.write_all(b"GET /a HTTP/1.1\r\nHost: t\r\n\r\nGET /b HTTP/1.1\r\nHost: t\r\n\r\n")
        .unwrap();
    assert!(String::from_utf8_lossy(&read_one(&mut s).1).starts_with("n=4"));
    assert!(String::from_utf8_lossy(&read_one(&mut s).1).starts_with("n=5"));
    // `Connection: close` ends it
    s.write_all(b"GET /c HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n")
        .unwrap();
    let (head, _) = read_one(&mut s);
    assert!(head.contains("Connection: close"), "{head}");
    let mut rest = Vec::new();
    s.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
}

#[test]
fn a_chunked_body_is_decoded_and_100_continue_is_answered() {
    let server = start("chunked", SERVER, &[]);
    let r = exchange(
        &server,
        "POST /x HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\nTrailer: x\r\n\r\n",
    );
    assert!(r.ends_with("body=hello world"), "{r}");
    // both framings at once are refused
    let r = exchange(
        &server,
        "POST /x HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\nConnection: close\r\n\r\n0\r\n\r\n",
    );
    assert!(r.starts_with("HTTP/1.1 400"), "{r}");
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.write_all(
        b"POST /x HTTP/1.1\r\nHost: t\r\nExpect: 100-continue\r\nContent-Length: 2\r\nConnection: close\r\n\r\n",
    )
    .unwrap();
    let mut first = [0u8; 25];
    s.read_exact(&mut first).unwrap();
    assert!(String::from_utf8_lossy(&first).starts_with("HTTP/1.1 100 Continue"));
    s.write_all(b"ok").unwrap();
    let mut rest = String::new();
    s.read_to_string(&mut rest).unwrap();
    assert!(rest.ends_with("body=ok"), "{rest}");
}

#[test]
fn bytes_go_in_and_out_unchanged() {
    let server = start("binary", SERVER, &[]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    let payload: Vec<u8> = vec![0xff, 0xfe, 0x00, b'a'];
    let mut req = format!(
        "POST /bytes HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        payload.len()
    )
    .into_bytes();
    req.extend_from_slice(&payload);
    s.write_all(&req).unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).unwrap();
    assert!(
        out.ends_with(&[0xff, 0xfe, 0x00, b'a', 0x00, 0xff]),
        "{out:?}"
    );
    assert!(String::from_utf8_lossy(&out).contains("application/octet-stream"));
}

#[test]
fn a_multipart_form_is_split_into_parts() {
    let server = start("multipart", SERVER, &[]);
    let body = "--B\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n12345\r\n--B\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\r\nabc\r\n--B--\r\n";
    let r = exchange(
        &server,
        &format!(
            "POST /upload HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Type: multipart/form-data; boundary=B\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(r.ends_with("a:5;f:3;"), "{r}");
    let r = exchange(
        &server,
        "POST /upload HTTP/1.1\r\nHost: t\r\nConnection: close\r\nContent-Length: 1\r\n\r\nx",
    );
    assert!(r.starts_with("HTTP/1.1 400"), "{r}");
}

#[test]
fn an_answer_can_be_streamed_and_events_are_framed() {
    let server = start("stream", SERVER, &[]);
    let r = exchange(&server, "GET /stream HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.contains("Transfer-Encoding: chunked"), "{r}");
    assert!(
        r.contains("6\r\npart1\n\r\n6\r\npart2\n\r\n0\r\n\r\n"),
        "{r}"
    );
    let r = exchange(&server, "GET /events HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(r.contains("Content-Type: text/event-stream"), "{r}");
    assert!(r.contains("event: t\ndata: tick 1\n\n"), "{r}");
    assert!(r.contains("data: tick 3\n\n"), "{r}");
    assert!(!r.contains("tick 4"), "{r}");
    // the server goes on afterwards
    let r = exchange(
        &server,
        "GET /x HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
}

#[test]
fn text_answers_are_compressed_only_when_asked_for_and_wanted() {
    let plain = start("nogzip", SERVER, &[]);
    let r = exchange(
        &plain,
        "GET /big HTTP/1.1\r\nHost: t\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n",
    );
    assert!(!r.contains("Content-Encoding"), "off by default");
    let server = start("gzip", SERVER, &["--compress"]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.write_all(
        b"GET /big HTTP/1.1\r\nHost: t\r\nAccept-Encoding: gzip, br\r\nConnection: close\r\n\r\n",
    )
    .unwrap();
    let (head, body) = read_one(&mut s);
    assert!(head.contains("Content-Encoding: gzip"), "{head}");
    assert!(head.contains("Vary: Accept-Encoding"), "{head}");
    assert_eq!(&body[..2], &[0x1f, 0x8b], "gzip magic");
    assert!(body.len() < 9600 / 4, "{} bytes", body.len());
    // not asked: not compressed
    let r = exchange(
        &server,
        "GET /big HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(!r.contains("Content-Encoding"), "{r}");
}

#[test]
fn the_limits_can_be_changed() {
    let server = start(
        "limits",
        APP,
        &[
            "--max-body",
            "1k",
            "--max-headers",
            "512",
            "--read-timeout",
            "1",
        ],
    );
    let big_body = format!(
        "POST /x HTTP/1.1\r\nHost: t\r\nContent-Length: {}\r\n\r\n",
        2048
    );
    assert!(exchange(&server, &big_body).starts_with("HTTP/1.1 413"));
    let small_body = "POST /x HTTP/1.1\r\nHost: t\r\nContent-Length: 2\r\n\r\nhi";
    assert!(exchange(&server, small_body).starts_with("HTTP/1.1 201"));
    let long_header = format!(
        "GET / HTTP/1.1\r\nHost: t\r\nX-Pad: {}\r\n\r\n",
        "a".repeat(2000)
    );
    assert!(exchange(&server, &long_header).starts_with("HTTP/1.1 431"));
    // a client that starts a request and never ends it is cut after --read-timeout
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: t\r\n").unwrap();
    let started = std::time::Instant::now();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 408"), "{out}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn connections_are_limited_by_option() {
    let server = start("conns", APP, &["--max-connections", "1"]);
    // the first connection stays open and silent: it takes the only slot
    let _held = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    // the refusal is written as soon as the connection is accepted
    let mut refused = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    let mut out = String::new();
    let _ = refused.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 503"), "{out}");
}

#[test]
fn a_bad_limit_is_refused_at_start() {
    for (option, value) in [
        ("--max-body", "lots"),
        ("--max-connections", "0"),
        ("--read-timeout", "0"),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(["serve", "x.q", option, value])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(64), "{option} {value}");
    }
}

/// Damaged requests of every kind: the server answers or closes, and still answers afterwards.
#[test]
fn damaged_requests_do_not_stop_the_server() {
    let server = start(
        "damaged",
        APP,
        &[
            "--max-body",
            "4k",
            "--max-headers",
            "2k",
            "--read-timeout",
            "1",
        ],
    );
    let good = "POST /x HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhello";
    let mut seed: u64 = 0x2545F4914F6CDD1D;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"\r\n\r\n".to_vec(),
        b"GET\r\n\r\n".to_vec(),
        b"GET / HTTP/9.9\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nHost\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\n: no-name\r\n\r\n".to_vec(),
        b"POST /x HTTP/1.1\r\nContent-Length: -1\r\n\r\n".to_vec(),
        b"POST /x HTTP/1.1\r\nContent-Length: 99999999999999999999\r\n\r\n".to_vec(),
        b"POST /x HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab".to_vec(),
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nZZ\r\nabc\r\n0\r\n\r\n".to_vec(),
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFF\r\n".to_vec(),
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n0\r\n\r\n".to_vec(),
        b"GET /%zz%00%ff HTTP/1.1\r\nHost: t\r\n\r\n".to_vec(),
        b"GET /\xff\xfe HTTP/1.1\r\nHost: t\r\n\r\n".to_vec(),
        b"GET /ws HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: x\r\n\r\n".to_vec(),
        vec![0u8; 3000],
        b"G".repeat(5000),
    ];
    for _ in 0..150 {
        let mut bytes = good.as_bytes().to_vec();
        for _ in 0..1 + next() % 4 {
            let at = (next() as usize) % (bytes.len() + 1);
            match next() % 3 {
                0 if at < bytes.len() => {
                    bytes.remove(at);
                }
                1 => bytes.insert(at, (next() % 256) as u8),
                _ => bytes.truncate(at),
            }
        }
        cases.push(bytes);
    }
    for case in &cases {
        if let Ok(mut s) = TcpStream::connect(("127.0.0.1", server.port)) {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
            let _ = s.write_all(case);
            let _ = s.shutdown(std::net::Shutdown::Write);
            let mut sink = Vec::new();
            let _ = s.read_to_end(&mut sink);
        }
    }
    // still alive, and still right
    assert!(exchange(&server, good).starts_with("HTTP/1.1 201"));
}

const FAILING: &str = r#"
let db = Db.connect("sqlite://:memory:").unwrap()
db.execute("CREATE TABLE t (id INT)", []).unwrap()

fun helper(n: int) -> int
  if n == 3 then
    panic("exploded <here>")
  end
  n
end

fun handle(req: Request) -> Response
  if req.url == "/ok" then
    let rows = db.query("SELECT id FROM t WHERE id = ?", [Json.number(1.0)])
    return Response { status: 200, body: "fine" }
  end
  for i in [1, 2, 3, 4] do
    db.query("SELECT id FROM t WHERE id = ?", [Json.number(i as float)])
  end
  Response { status: 200, body: "{helper(3)}" }
end
"#;

fn get_with(server: &Server, path: &str, accept: &str) -> String {
    exchange(
        server,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: t\r\nAccept: {accept}\r\nCookie: qs=topsecret\r\n\r\n"
        ),
    )
}

#[test]
fn dev_shows_a_page_for_a_browser_json_for_a_client_and_text_otherwise() {
    let server = start("devpage", FAILING, &["--dev"]);
    let html = get_with(&server, "/boom", "text/html,application/xhtml+xml");
    assert!(html.starts_with("HTTP/1.1 500"), "{html}");
    assert!(html.contains("text/html"), "{html}");
    assert!(html.contains("exploded &lt;here&gt;"), "{html}");
    assert!(html.contains("class=\"line hit\""), "{html}");
    assert!(html.contains("N+1?"), "{html}");
    assert!(html.contains("/boom"), "{html}");
    assert!(
        !html.contains("topsecret"),
        "the cookie must not be shown: {html}"
    );

    let json = get_with(&server, "/boom", "application/json");
    assert!(json.contains("application/json"), "{json}");
    let body = json.split("\r\n\r\n").nth(1).unwrap();
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("exploded <here>")
    );
    assert_eq!(v["queries"].as_array().unwrap().len(), 4);
    assert_eq!(v["request"]["method"], "GET");
    assert!(!body.contains("topsecret"), "{body}");

    let text = get_with(&server, "/boom", "*/*");
    assert!(
        text.contains("text/plain") && text.contains("Internal Server Error"),
        "{text}"
    );
    assert!(text.contains("exploded"), "{text}");

    // a request that works carries how long its SQL took
    let ok = get_with(&server, "/ok", "text/html");
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains("Server-Timing: db;dur="), "{ok}");
}

#[test]
fn without_dev_the_page_shows_nothing_of_the_program() {
    let server = start("nodev", FAILING, &[]);
    for accept in ["text/html", "application/json", "*/*"] {
        let r = get_with(&server, "/boom", accept);
        assert!(r.starts_with("HTTP/1.1 500"), "{r}");
        assert!(r.ends_with("Internal Server Error"), "{accept}: {r}");
        assert!(!r.contains("exploded") && !r.contains("handle"), "{r}");
        assert!(!r.contains("Server-Timing"), "{r}");
    }
}

#[test]
fn dev_is_refused_on_a_public_address() {
    let file = app_file("devpublic", APP);
    let run = |extra: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("serve")
            .arg(&file)
            .args(["--port", &free_port().to_string(), "--dev"])
            .args(extra)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let refused = run(&["--bind", "0.0.0.0"]).wait_with_output().unwrap();
    assert_eq!(refused.status.code(), Some(64));
    let out = String::from_utf8_lossy(&refused.stderr);
    assert!(out.contains("only allowed on localhost"), "{out}");
    // --dev-public says it is meant; a started server never exits by itself, so end it
    let mut allowed = run(&["--bind", "0.0.0.0", "--dev-public"]);
    let mut line = String::new();
    BufReader::new(allowed.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let _ = allowed.kill();
    let _ = allowed.wait();
    assert!(line.contains("serving"), "{line}");
}

#[test]
fn a_failure_is_one_json_line_with_its_stack_in_production() {
    let file = app_file("jsonlog", FAILING);
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("serve")
        .arg(&file)
        .args([
            "--port",
            &port.to_string(),
            "--log-format",
            "json",
            "--no-access-log",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("serving"), "{line}");
    let answer = exchange_port(port, "GET /boom HTTP/1.1\r\nHost: t\r\n\r\n");
    assert!(answer.ends_with("Internal Server Error"), "{answer}");
    line.clear();
    reader.read_line(&mut line).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(v["level"], "error", "{line}");
    assert!(
        v["message"].as_str().unwrap().contains("exploded"),
        "{line}"
    );
    assert_eq!(v["context"]["code"], "R050", "{line}");
    assert_eq!(v["context"]["url"], "/boom", "{line}");
    let stack = v["context"]["stack"].as_array().unwrap();
    assert!(
        stack.iter().any(|f| f.as_str().unwrap().contains("helper")),
        "{line}"
    );
    // the source code is not in the log
    assert!(!line.contains("panic(\"exploded"), "{line}");
}

#[test]
fn a_file_is_sent_from_the_disk_with_its_length_and_type() {
    let dir = std::env::temp_dir().join(format!("qlang-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(dir.join("data.bin"), &big).unwrap();
    std::fs::write(dir.join("page.json"), "{\"a\":1}").unwrap();
    let source = format!(
        r#"
fun handle(req: Request) -> Response
  if req.path == "/big" then
    return Response.file("{d}/data.bin")
  end
  if req.path == "/json" then
    return Response.file("{d}/page.json")
  end
  if req.path == "/save" then
    return Response.download("{d}/page.json", "my \"report\".json")
  end
  if req.path == "/gone" then
    return Response.file("{d}/missing.bin")
  end
  Response.text("ok")
end
"#,
        d = dir.display().to_string().replace('\\', "/")
    );
    let server = start("sendfile", &source, &[]);
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.write_all(b"GET /big HTTP/1.1\r\nHost: t\r\n\r\n")
        .unwrap();
    let (head, body) = read_one(&mut s);
    assert!(head.contains("Content-Length: 300000"), "{head}");
    assert!(
        head.contains("Content-Type: application/octet-stream"),
        "{head}"
    );
    assert_eq!(body, big);
    // the same connection serves the next request
    s.write_all(b"GET /json HTTP/1.1\r\nHost: t\r\n\r\n")
        .unwrap();
    let (head, body) = read_one(&mut s);
    assert!(head.contains("Content-Type: application/json"), "{head}");
    assert_eq!(body, b"{\"a\":1}");
    s.write_all(b"HEAD /big HTTP/1.1\r\nHost: t\r\n\r\n")
        .unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    assert!(String::from_utf8_lossy(&head).contains("Content-Length: 300000"));
    drop(s);
    let saved = exchange(
        &server,
        "GET /save HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(
        saved.contains("Content-Disposition: attachment; filename=\"my report.json\""),
        "{}",
        &saved[..300.min(saved.len())]
    );
    let gone = exchange(
        &server,
        "GET /gone HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(gone.starts_with("HTTP/1.1 404"), "{gone}");
}

#[test]
fn a_file_outside_of_root_is_refused() {
    let base = std::env::temp_dir().join(format!("qlang-fileroot-{}", std::process::id()));
    // the program lives in this folder, which is the root
    let inside =
        std::env::temp_dir().join(format!("qlang-serve-test-{}-fileroot", std::process::id()));
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join("secret.txt"), "secret").unwrap();
    std::fs::write(inside.join("ok.txt"), "fine").unwrap();
    let source = format!(
        r#"
fun handle(req: Request) -> Response
  if req.path == "/secret" then
    return Response.file("{b}/secret.txt")
  end
  Response.file("{i}/ok.txt")
end
"#,
        b = base.display().to_string().replace('\\', "/"),
        i = inside.display().to_string().replace('\\', "/")
    );
    let root = inside.display().to_string();
    let server = start("fileroot", &source, &["--root", &root]);
    let secret = exchange(
        &server,
        "GET /secret HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(secret.starts_with("HTTP/1.1 403"), "{secret}");
    assert!(!secret.contains("secret."));
    let fine = exchange(
        &server,
        "GET /ok HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n",
    );
    assert!(
        fine.starts_with("HTTP/1.1 200") && fine.ends_with("fine"),
        "{fine}"
    );
}
