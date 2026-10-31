//! `--max-workers`: interpreters are added while all are busy, and leave when idle.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

const APP: &str =
    "fun handle(req: Request) -> Response\n  time.sleep(600)\n  Response.text(\"done\")\nend\n";

fn start(name: &str, extra: &[&str]) -> Server {
    let dir = std::env::temp_dir().join(format!("qlang-elastic-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("app.q");
    std::fs::write(&file, APP).unwrap();
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
            .args(extra)
            .env("QLANG_WORKER_IDLE_MS", "300")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stderr.take().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.contains("serving") {
            let log: Arc<Mutex<String>> = Arc::new(Mutex::new(line));
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

fn get(port: u16) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

fn burst(port: u16, n: usize) -> Duration {
    let started = Instant::now();
    let handles: Vec<_> = (0..n)
        .map(|_| std::thread::spawn(move || get(port)))
        .collect();
    for h in handles {
        assert!(h.join().unwrap().contains("done"));
    }
    started.elapsed()
}

#[test]
fn slow_requests_overlap_when_workers_may_be_added_and_the_extra_ones_leave() {
    let server = start("grow", &["--workers", "1", "--max-workers", "4"]);
    let took = burst(server.port, 4);
    // one interpreter would need 4 x 600 ms
    assert!(took < Duration::from_millis(2000), "{took:?}");
    assert!(
        server
            .log
            .lock()
            .unwrap()
            .contains("(up to 4 while they are busy)")
    );
    assert!(
        server
            .log
            .lock()
            .unwrap()
            .contains("started (every worker was busy)")
    );
    // nothing comes: the added ones end
    let until = Instant::now() + Duration::from_secs(10);
    while !server.log.lock().unwrap().contains("stopped (idle)") {
        assert!(
            Instant::now() < until,
            "no worker left: {}",
            server.log.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // and the server still answers
    assert!(get(server.port).contains("done"));
}

#[test]
fn without_max_workers_the_workers_stay_what_they_are() {
    let server = start("fixed", &["--workers", "1"]);
    let took = burst(server.port, 3);
    assert!(took >= Duration::from_millis(1700), "{took:?}");
    assert!(
        !server
            .log
            .lock()
            .unwrap()
            .contains("started (every worker was busy)")
    );
}

#[test]
fn max_workers_below_workers_is_refused() {
    let dir = std::env::temp_dir().join(format!("qlang-elastic-{}-bad", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.q"), APP).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["serve", "app.q", "--workers", "4", "--max-workers", "2"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&o.stderr).contains("smaller than --workers"));
}
