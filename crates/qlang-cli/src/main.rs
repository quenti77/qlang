//! The `qlang` command line tool: runs programs in a terminal, speaks JSON
//! for other programs (PHP, Python...), and can serve that JSON over HTTP.

mod db;
mod json;
mod serve;

use qlang_core::check::compile;
use qlang_core::host::{Capabilities, Host, HostError, HttpRequest, HttpResponse, Limits, SqlRow, SqlValue};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
qlang - a small, readable language

USAGE:
    qlang run <file.q> [--max-steps N] [--root DIR] [--seed N]
                                         run a program (imports may live anywhere, or only
                                         under DIR with --root; the same --seed gives the
                                         same random numbers)
    qlang check <file.q>                 check a program without running it
    qlang check --json                   same, with a JSON request on stdin (used by editors)
    qlang run --json                     read a JSON request on stdin, write a JSON response
    qlang serve [--port 8080] [--bind 127.0.0.1]
                                         serve the JSON protocol over HTTP (POST /run)
    qlang --help | --version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // run on a big stack: the interpreter recurses for nested calls
    let handle = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || real_main(args))
        .expect("cannot start the interpreter thread");
    match handle.join() {
        Ok(code) => code,
        Err(_) => ExitCode::from(101),
    }
}

fn real_main(args: Vec<String>) -> ExitCode {
    let mut it = args.iter().map(String::as_str);
    match it.next() {
        Some("run") => {
            let rest: Vec<&str> = it.collect();
            if rest.contains(&"--json") {
                json::run_stdin(false)
            } else {
                run_file(&rest, true)
            }
        }
        Some("check") => {
            let rest: Vec<&str> = it.collect();
            if rest.contains(&"--json") { json::run_stdin(true) } else { run_file(&rest, false) }
        }
        Some("serve") => serve::serve(&it.collect::<Vec<_>>()),
        Some("--version") | Some("-V") => {
            println!("qlang {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(64)
        }
    }
}

/// A host connected to the terminal and the disk. Imports can reach any `.q` file the
/// user can read (like `require` in Node), unless `--root` confines them to a directory.
struct TerminalHost {
    root: Option<PathBuf>,
    /// State of the random generator: `--seed N`, or taken from the system.
    rng: u64,
    /// Open SQL connections, for `Db.connect`.
    db: db::SqlConns,
}

/// Random bits from the system (`/dev/urandom`, or the keys std picks for its hash tables).
fn system_entropy() -> u64 {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).is_ok() {
        return u64::from_le_bytes(bytes);
    }
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new().build_hasher().finish()
}

impl Host for TerminalHost {
    fn print(&mut self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes());
    }

    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line.trim_end_matches(['\n', '\r']).to_string())),
            Err(e) => Err(HostError::Other(e.to_string())),
        }
    }

    fn load_module(&mut self, path: &str) -> Result<String, String> {
        // only qlang sources: importing any file would show its lines in error messages
        if !path.ends_with(".q") {
            return Err(format!("\"{path}\" is not a qlang file (only .q files can be imported)"));
        }
        let full = Path::new(path).canonicalize().map_err(|_| format!("file \"{path}\" not found"))?;
        if let Some(root) = &self.root
            && !full.starts_with(root) {
                return Err(format!("\"{path}\" is outside of --root {}", root.display()));
            }
        std::fs::read_to_string(&full).map_err(|e| e.to_string())
    }

    fn canonical_path(&mut self, path: &str) -> String {
        // the same file reached through `..` or a symbolic link is the same module
        Path::new(path)
            .canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    }

    fn now_ms(&mut self) -> Result<i64, HostError> {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        since.map(|d| d.as_millis() as i64).map_err(|e| HostError::Other(e.to_string()))
    }

    fn random_u64(&mut self) -> Result<u64, HostError> {
        Ok(qlang_core::host::splitmix64(&mut self.rng))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { read: true, time: true, random: true, http: true, sql: true }
    }

    fn sql_connect(&mut self, url: &str) -> Result<u64, HostError> {
        self.db.connect(url)
    }

    fn sql_query(&mut self, conn: u64, sql: &str, params: Vec<SqlValue>) -> Result<Vec<SqlRow>, HostError> {
        self.db.query(conn, sql, params)
    }

    fn sql_execute(&mut self, conn: u64, sql: &str, params: Vec<SqlValue>) -> Result<u64, HostError> {
        self.db.execute(conn, sql, params)
    }

    fn sql_close(&mut self, conn: u64) -> Result<(), HostError> {
        self.db.close(conn)
    }

    fn http_send(&mut self, req: HttpRequest) -> Result<HttpResponse, HostError> {
        let mut r = ureq::request(&req.method, &req.url);
        for (name, value) in &req.headers {
            r = r.set(name, value);
        }
        let result = match &req.body {
            Some(body) => r.send_string(body),
            None => r.call(),
        };
        match result {
            Ok(resp) => {
                let status = resp.status() as i64;
                let body = resp.into_string().map_err(|e| HostError::Other(e.to_string()))?;
                Ok(HttpResponse { status, body })
            }
            // a non-2xx/3xx status is still a response, not a failure
            Err(ureq::Error::Status(status, resp)) => {
                let body = resp.into_string().map_err(|e| HostError::Other(e.to_string()))?;
                Ok(HttpResponse { status: status as i64, body })
            }
            Err(ureq::Error::Transport(t)) => Err(HostError::Other(t.to_string())),
        }
    }
}

fn run_file(args: &[&str], execute: bool) -> ExitCode {
    let mut file: Option<&str> = None;
    let mut root: Option<PathBuf> = None;
    let mut seed: Option<u64> = None;
    let mut limits = Limits::default();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--max-steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => limits.max_steps = n,
                    None => {
                        eprintln!("--max-steps needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--seed" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => seed = Some(n),
                    None => {
                        eprintln!("--seed needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--root" => {
                i += 1;
                match args.get(i).map(|d| Path::new(d).canonicalize()) {
                    Some(Ok(dir)) if dir.is_dir() => root = Some(dir),
                    _ => {
                        eprintln!("--root needs an existing directory");
                        return ExitCode::from(64);
                    }
                }
            }
            a if a.starts_with('-') => {
                eprintln!("unknown option `{a}`");
                return ExitCode::from(64);
            }
            a => file = Some(a),
        }
        i += 1;
    }
    let Some(file) = file else {
        eprintln!("missing file\n\n{USAGE}");
        return ExitCode::from(64);
    };
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            return ExitCode::from(66);
        }
    };
    if let (Some(root), Ok(full)) = (&root, Path::new(file).canonicalize())
        && !full.starts_with(root) {
            eprintln!("{file} is outside of --root {}", root.display());
            return ExitCode::from(64);
        }
    let mut host = TerminalHost { root, rng: seed.unwrap_or_else(system_entropy), db: db::SqlConns::default() };
    let program = match compile(file, &source, &mut host) {
        Ok(p) => p,
        Err(e) => {
            eprint!("{}", e.render());
            let n = e.diagnostics.len();
            eprintln!("\n{n} error{} found", if n == 1 { "" } else { "s" });
            return ExitCode::from(1);
        }
    };
    for w in &program.warnings {
        eprint!("{}", w.render(&program.sources));
    }
    if !execute {
        eprintln!("ok");
        return ExitCode::SUCCESS;
    }
    let result = program.run(&mut host, limits);
    let _ = std::io::stdout().flush();
    match result.error {
        None => ExitCode::SUCCESS,
        Some(d) => {
            eprint!("{}", d.render(&program.sources));
            ExitCode::from(2)
        }
    }
}
