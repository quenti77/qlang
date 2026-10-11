//! A tiny HTTP server for the JSON protocol: `POST /run` and `GET /health`.
//!
//! It is deliberately minimal (no TLS, no keep-alive). Put it behind a
//! reverse proxy and bind it to localhost unless you know what you are doing.

use crate::json::{self, Ceiling};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const MAX_BODY: usize = 1 << 20;
const MAX_CONNECTIONS: usize = 16;

pub fn serve(args: &[&str]) -> ExitCode {
    let mut port: u16 = 8080;
    let mut bind = "127.0.0.1".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
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
            other => {
                eprintln!("unknown option `{other}`");
                return ExitCode::from(64);
            }
        }
        i += 1;
    }
    let listener = match TcpListener::bind((bind.as_str(), port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on {bind}:{port}: {e}");
            return ExitCode::from(74);
        }
    };
    eprintln!("qlang serving on http://{bind}:{port} (POST /run)");
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            let _ = respond(stream, 503, "{\"error\":\"server busy\"}");
            continue;
        }
        let slot = Slot::take(&active);
        let spawned = std::thread::Builder::new().stack_size(256 << 20).spawn(move || {
            // the slot is freed when the thread ends, whatever happens (including a panic)
            let _slot = slot;
            let reply = stream.try_clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle_connection(stream)));
            if outcome.is_err()
                && let Ok(s) = reply {
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

fn respond(mut stream: TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn handle_connection(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(p) = find(&buf, b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 64 * 1024 {
            let _ = respond(stream, 400, "{\"error\":\"headers too large\"}");
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let content_length = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0);

    match (method, path) {
        ("GET", "/health") => {
            let _ = respond(stream, 200, "{\"status\":\"ok\"}");
        }
        ("POST", "/run") => {
            if content_length > MAX_BODY {
                let _ = respond(stream, 413, "{\"error\":\"request too large\"}");
                return;
            }
            let mut body: Vec<u8> = buf[header_end + 4..].to_vec();
            while body.len() < content_length {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => body.extend_from_slice(&chunk[..n]),
                }
            }
            body.truncate(content_length);
            let text = String::from_utf8_lossy(&body).to_string();
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
