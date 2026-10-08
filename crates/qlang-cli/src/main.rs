//! The `qlang` command line tool: runs programs in a terminal, speaks JSON
//! for other programs (PHP, Python...), and can serve that JSON over HTTP.

mod json;
mod serve;

use qlang_core::check::compile;
use qlang_core::host::{Capabilities, Host, HostError, Limits};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
qlang - a small, readable language

USAGE:
    qlang run <file.q> [--max-steps N]   run a program
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

/// A host connected to the terminal and the disk, limited to the entry
/// file's directory for imports.
struct TerminalHost {
    root: PathBuf,
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
        let full = Path::new(path).canonicalize().map_err(|_| format!("file \"{path}\" not found"))?;
        if !full.starts_with(&self.root) {
            return Err("modules must be inside the directory of the main file".to_string());
        }
        std::fs::read_to_string(&full).map_err(|e| e.to_string())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { read: true }
    }
}

fn run_file(args: &[&str], execute: bool) -> ExitCode {
    let mut file: Option<&str> = None;
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
    let root = Path::new(file)
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut host = TerminalHost { root };
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
