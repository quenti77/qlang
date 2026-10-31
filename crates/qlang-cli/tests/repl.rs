//! `qlang repl`, driven through a pipe.

use std::io::Write;
use std::process::{Command, Stdio};

fn repl(input: &str) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("repl")
        .env("HOME", std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let o = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn definitions_stay_and_expressions_show_their_value() {
    let (out, err) = repl(
        "let x = 2\nx * 21\nfun double(n: int) -> int\n  n * 2\nend\ndouble(x)\nprint(\"hi\")\n",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, ["42", "4", "hi"], "{out}\n{err}");
}

#[test]
fn an_error_is_shown_and_the_session_goes_on() {
    let (out, err) = repl("nothing_here\n1 + 1\n");
    assert!(err.contains("nothing_here"), "{err}");
    assert!(out.lines().any(|l| l == "2"), "{out}");
}

#[test]
fn a_failed_definition_is_not_kept() {
    let (out, err) = repl("let bad = 1 div 0\nlet ok = 5\nok\n");
    assert!(!err.is_empty(), "{err}");
    assert!(out.lines().any(|l| l == "5"), "{out}");
}
