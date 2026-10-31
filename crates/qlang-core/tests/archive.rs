//! Compression and time zones are the `qlang` command's: a host without them reports their use
//! before running.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
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
fn a_host_without_the_extras_refuses_them_when_compiling() {
    for src in [
        "print(Gzip.compress(\"a\".to_bytes()).len())",
        "print(Zip.read(\"a\".to_bytes()).is_ok())",
        "print(Zone.of(\"Europe/Paris\").is_ok())",
    ] {
        let out = exec(src);
        assert!(
            out.starts_with("COMPILE: T250") && out.contains("only offered by the `qlang` command"),
            "{src}: {out}"
        );
    }
}

#[test]
fn shared_numbers_need_the_qlang_command_too() {
    let out = exec("print(Shared.add(\"n\", 1).is_ok())");
    assert!(
        out.starts_with("COMPILE: T250") && out.contains("only offered by the `qlang` command"),
        "{out}"
    );
}

#[test]
fn tracing_needs_the_qlang_command_too() {
    let out = exec("print(Trace.id())");
    assert!(
        out.starts_with("COMPILE: T250")
            && out.contains("tracing is only offered by the `qlang` command"),
        "{out}"
    );
}

#[test]
fn workers_need_the_qlang_command_too() {
    let out = exec("print(Worker.spawn(\"a.q\", Json.null()).is_ok())");
    assert!(
        out.starts_with("COMPILE: T250") && out.contains("only offered by the `qlang` command"),
        "{out}"
    );
}
