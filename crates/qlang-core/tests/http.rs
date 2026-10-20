//! `http.send`: the host performs the request; a connection failure becomes a `Result::err`,
//! never a crash.

use qlang_core::check::compile;
use qlang_core::host::{HttpResponse, Limits, MemoryHost};
use std::collections::HashMap;

fn host_with(responses: HashMap<(String, String), Result<HttpResponse, String>>) -> MemoryHost {
    MemoryHost {
        http: Some(responses),
        ..MemoryHost::default()
    }
}

fn canned(method: &str, url: &str, result: Result<HttpResponse, String>) -> MemoryHost {
    let mut m = HashMap::new();
    m.insert((method.to_string(), url.to_string()), result);
    host_with(m)
}

/// Output of the program, or `CODE: message` (compile or run error).
fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return format!(
                        "COMPILE: {}",
                        e.diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.code, d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn http_send_needs_the_network_capability() {
    let out = exec(
        "let r = http.send(Request { method: \"GET\", url: \"https://example.com\" })",
        MemoryHost::default(),
    );
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn http_send_returns_the_status_and_body_on_success() {
    let host = canned(
        "GET",
        "https://example.com",
        Ok(HttpResponse {
            status: 200,
            headers: vec![],
            body: "hello".to_string(),
        }),
    );
    let out = exec(
        r#"
let r = http.send(Request { method: "GET", url: "https://example.com" })
let resp = r.unwrap()
print(resp.status)
print(resp.body)
"#,
        host,
    );
    assert_eq!(out, "200\nhello\n");
}

#[test]
fn http_send_wraps_a_connection_failure_in_err_instead_of_crashing() {
    let host = canned(
        "GET",
        "https://down.example.com",
        Err("connection refused".to_string()),
    );
    let out = exec(
        r#"
let r = http.send(Request { method: "GET", url: "https://down.example.com" })
if r.is_err() then
    print(r.unwrap_err().message)
end
"#,
        host,
    );
    assert_eq!(out, "connection refused\n");
}

#[test]
fn request_headers_and_body_default_to_empty_and_none() {
    let host = canned(
        "POST",
        "https://example.com/items",
        Ok(HttpResponse {
            status: 201,
            headers: vec![],
            body: "created".to_string(),
        }),
    );
    let out = exec(
        r#"
let req = Request { method: "POST", url: "https://example.com/items" }
print(req.headers.len())
print(req.body == none)
print(http.send(req).unwrap().status)
"#,
        host,
    );
    assert_eq!(out, "0\ntrue\n201\n");
}
