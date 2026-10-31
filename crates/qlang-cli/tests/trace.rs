//! Tracing: `qlang serve --otlp-endpoint`, with a collector and a downstream service played by the test.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Got {
    head: String,
    body: String,
}

/// A server that records what it receives and answers `200 ok`.
fn fake_server() -> (u16, Arc<Mutex<Vec<Got>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let got: Arc<Mutex<Vec<Got>>> = Arc::default();
    let seen = got.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                seen.lock().unwrap().push(Got {
                    head,
                    body: String::from_utf8_lossy(&body).into_owned(),
                });
                let mut stream = stream;
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
            });
        }
    });
    (port, got)
}

struct Server {
    child: Child,
    port: u16,
    log: Arc<Mutex<String>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(name: &str, source: &str, extra: &[&str]) -> Server {
    let dir = std::env::temp_dir().join(format!("qlang-trace-{}-{name}", std::process::id()));
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
            .args(["--port", &port.to_string(), "--no-access-log"])
            .args(["--log-format", "json"])
            .args(extra)
            .env("QLANG_OTLP_FLUSH_MS", "50")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stderr.take().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.contains("serving") {
            let log: Arc<Mutex<String>> = Arc::default();
            let sink = log.clone();
            std::thread::spawn(move || {
                let mut text = String::new();
                while reader.read_line(&mut text).unwrap_or(0) > 0 {
                    sink.lock().unwrap().push_str(&text);
                    text.clear();
                }
            });
            return Server { child, port, log };
        }
        let _ = child.wait();
    }
    panic!("no free port");
}

fn get(port: u16, extra_headers: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(
        format!(
            "GET /orders?secret=1 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra_headers}\r\n"
        )
        .as_bytes(),
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn app(downstream: u16) -> String {
    format!(
        r#"
fun handle(req: Request) -> Response
  Log.info("handling")
  let n = Trace.span("work", fun() -> int
    Trace.attr("items", "3")
    let r = http.send(Request {{ method: "GET", url: "http://127.0.0.1:{downstream}/stock?token=abc" }})
    1
  end)
  Response.text("trace={{Trace.id()}} n={{n}}")
end
"#
    )
}

fn spans_of(collector: &Arc<Mutex<Vec<Got>>>) -> Vec<Value> {
    let mut out = Vec::new();
    for g in collector.lock().unwrap().iter() {
        assert!(g.head.starts_with("POST /v1/traces "), "{}", g.head);
        let v: Value = serde_json::from_str(&g.body).unwrap();
        for rs in v["resourceSpans"].as_array().unwrap() {
            for ss in rs["scopeSpans"].as_array().unwrap() {
                out.extend(ss["spans"].as_array().unwrap().iter().cloned());
            }
        }
    }
    out
}

#[test]
fn a_request_continues_the_callers_trace_and_its_spans_are_exported() {
    let (collector_port, collector) = fake_server();
    let (down_port, downstream) = fake_server();
    let server = start(
        "continue",
        &app(down_port),
        &[
            "--otlp-endpoint",
            &format!("http://127.0.0.1:{collector_port}"),
            "--service-name",
            "orders",
        ],
    );
    let trace = "4bf92f3577b34da6a3ce929d0e0e4736";
    let answer = get(
        server.port,
        &format!("traceparent: 00-{trace}-00f067aa0ba902b7-01\r\n"),
    );
    assert!(answer.contains(&format!("trace={trace} n=1")), "{answer}");

    wait_for("the spans", || spans_of(&collector).len() >= 3);
    let spans = spans_of(&collector);
    assert!(spans.iter().all(|s| s["traceId"] == trace), "{spans:?}");
    let by_name = |n: &str| {
        spans
            .iter()
            .find(|s| s["name"] == n)
            .unwrap_or_else(|| panic!("no span {n}: {spans:?}"))
    };
    let server_span = by_name("GET /orders");
    assert_eq!(server_span["kind"], 2);
    assert_eq!(server_span["parentSpanId"], "00f067aa0ba902b7");
    let work = by_name("work");
    assert_eq!(work["parentSpanId"], server_span["spanId"]);
    assert!(work["attributes"].to_string().contains("items"));
    let client = by_name("HTTP GET");
    assert_eq!(client["kind"], 3);
    assert_eq!(client["parentSpanId"], work["spanId"]);
    // where the call went, without the query string (it may hold a secret)
    let url = client["attributes"].to_string();
    assert!(
        url.contains("http://127.0.0.1:") && url.contains("/stock") && !url.contains("token"),
        "{url}"
    );
    // the service is named in the resource
    let first: Value = serde_json::from_str(&collector.lock().unwrap()[0].body).unwrap();
    assert!(
        first["resourceSpans"][0]["resource"]
            .to_string()
            .contains("orders")
    );

    // the call the program made carries the trace on, with the span of the call as parent
    wait_for("the downstream call", || {
        !downstream.lock().unwrap().is_empty()
    });
    let head = downstream.lock().unwrap()[0].head.to_ascii_lowercase();
    let wanted = format!(
        "traceparent: 00-{trace}-{}-01",
        client["spanId"].as_str().unwrap()
    );
    assert!(head.contains(&wanted), "{head}\nwanted {wanted}");

    // and the log lines of the request say which trace they belong to
    wait_for("the log line", || {
        server.log.lock().unwrap().contains("handling")
    });
    let line = server
        .log
        .lock()
        .unwrap()
        .lines()
        .find(|l| l.contains("handling"))
        .unwrap()
        .to_string();
    let json: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(json["trace_id"], trace);
    assert_eq!(json["span_id"], server_span["spanId"]);
}

#[test]
fn a_request_without_a_trace_starts_one() {
    let (collector_port, collector) = fake_server();
    let (down_port, _downstream) = fake_server();
    let server = start(
        "fresh",
        &app(down_port),
        &[
            "--otlp-endpoint",
            &format!("http://127.0.0.1:{collector_port}/"),
        ],
    );
    let answer = get(server.port, "");
    let id = answer
        .split("trace=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap();
    assert_eq!(id.len(), 32, "{answer}");
    wait_for("the spans", || spans_of(&collector).len() >= 3);
    let spans = spans_of(&collector);
    assert!(spans.iter().all(|s| s["traceId"] == id));
    let root = spans.iter().find(|s| s["name"] == "GET /orders").unwrap();
    assert!(root.get("parentSpanId").is_none());
}

#[test]
fn without_a_collector_a_traceparent_is_still_passed_on() {
    let (down_port, downstream) = fake_server();
    let server = start("passthrough", &app(down_port), &[]);
    let trace = "0af7651916cd43dd8448eb211c80319c";
    let answer = get(
        server.port,
        &format!("traceparent: 00-{trace}-b7ad6b7169203331-01\r\n"),
    );
    assert!(answer.contains(&format!("trace={trace}")), "{answer}");
    wait_for("the downstream call", || {
        !downstream.lock().unwrap().is_empty()
    });
    let head = downstream.lock().unwrap()[0].head.to_ascii_lowercase();
    assert!(
        head.contains(&format!("traceparent: 00-{trace}-")),
        "{head}"
    );
    // no header, no collector: no trace at all
    let plain = get(server.port, "");
    assert!(plain.contains("trace= n=1"), "{plain}");
}

#[test]
fn a_collector_that_is_down_does_not_break_the_requests() {
    let dead = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (down_port, _d) = fake_server();
    let server = start(
        "dead",
        &app(down_port),
        &["--otlp-endpoint", &format!("http://127.0.0.1:{dead}")],
    );
    for _ in 0..3 {
        assert!(get(server.port, "").contains("n=1"));
    }
    wait_for("the warning", || {
        server.log.lock().unwrap().contains("did not accept")
    });
    // said once
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        server.log.lock().unwrap().matches("did not accept").count(),
        1
    );
}
