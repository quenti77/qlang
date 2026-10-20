//! `fs.read`/`fs.write` and `env.get`/`env.args`: the host owns the disk and the process
//! environment; a missing file or variable is never a crash.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};
use std::collections::HashMap;

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
fn fs_read_needs_the_fs_capability() {
    let out = exec(r#"let r = fs.read("x.txt")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn fs_write_needs_the_fs_capability() {
    let out = exec(r#"let r = fs.write("x.txt", "hi")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn fs_read_returns_the_files_content() {
    let mut files = HashMap::new();
    files.insert("greeting.txt".to_string(), "hello".to_string());
    let host = MemoryHost {
        fs: Some(files),
        ..MemoryHost::default()
    };
    let out = exec(r#"print(fs.read("greeting.txt").unwrap())"#, host);
    assert_eq!(out, "hello\n");
}

#[test]
fn fs_read_of_a_missing_file_is_a_result_err_not_a_crash() {
    let host = MemoryHost {
        fs: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
let r = fs.read("missing.txt")
print(r.is_err())
"#,
        host,
    );
    assert_eq!(out, "true\n");
}

#[test]
fn fs_write_then_read_round_trips() {
    let host = MemoryHost {
        fs: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
fs.write("out.txt", "written").unwrap()
print(fs.read("out.txt").unwrap())
"#,
        host,
    );
    assert_eq!(out, "written\n");
}

#[test]
fn env_get_needs_the_env_capability() {
    let out = exec(r#"let r = env.get("HOME")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn env_get_returns_none_for_an_unset_variable() {
    let host = MemoryHost {
        env: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
let v = env.get("NOPE")
print(v == none)
"#,
        host,
    );
    assert_eq!(out, "true\n");
}

#[test]
fn env_get_returns_a_set_variables_value() {
    let mut vars = HashMap::new();
    vars.insert("LANG".to_string(), "fr_FR".to_string());
    let host = MemoryHost {
        env: Some(vars),
        ..MemoryHost::default()
    };
    let out = exec(r#"print(env.get("LANG"))"#, host);
    assert_eq!(out, "fr_FR\n");
}

#[test]
fn env_args_returns_the_programs_own_arguments() {
    let host = MemoryHost {
        env: Some(HashMap::new()),
        cli_args: vec!["a".to_string(), "b".to_string()],
        ..MemoryHost::default()
    };
    let out = exec(r#"print(env.args())"#, host);
    assert_eq!(out, "[\"a\", \"b\"]\n");
}

#[test]
fn programs_that_never_use_fs_or_env_do_not_need_those_capabilities() {
    let out = exec(r#"print(1 + 1)"#, MemoryHost::default());
    assert_eq!(out, "2\n");
}
