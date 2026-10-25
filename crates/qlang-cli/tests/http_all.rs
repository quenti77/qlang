//! `http.send_all` against local servers.

use std::process::Command;

fn qlang() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qlang"))
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("qlang-par-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn http_send_all_runs_the_requests_at_the_same_time() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = stream;
                let mut buf = [0u8; 1024];
                let n = s.read(&mut buf).unwrap_or(0);
                let first = String::from_utf8_lossy(&buf[..n])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                std::thread::sleep(std::time::Duration::from_millis(700));
                let body = first.split(' ').nth(1).unwrap_or("").to_string();
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    let d = dir("par");
    let file = d.join("p.q");
    std::fs::write(
        &file,
        format!(
            "let started = time.now()\nlet answers = http.send_all([\n  Request {{ method: \"GET\", url: \"http://127.0.0.1:{port}/1\" }},\n  Request {{ method: \"GET\", url: \"http://127.0.0.1:{port}/2\" }},\n  Request {{ method: \"GET\", url: \"http://127.0.0.1:{port}/3\" }},\n])\nfor a in answers do\n  print(if a.is_ok() then a.unwrap().body else \"error\" end)\nend\nprint(time.now() - started < 1900)\n"
        ),
    )
    .unwrap();
    let out = qlang().arg("run").arg(&file).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "/1\n/2\n/3\ntrue\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_request_that_cannot_connect_is_an_error_among_the_answers() {
    let d = dir("refused");
    let file = d.join("r.q");
    std::fs::write(
        &file,
        "let answers = http.send_all([Request { method: \"GET\", url: \"http://127.0.0.1:1/x\" }])\nprint(answers.len())\nprint(answers[0].is_err())\n",
    )
    .unwrap();
    let out = qlang().arg("run").arg(&file).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1\ntrue\n");
}
