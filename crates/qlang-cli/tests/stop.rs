//! `Process.stopping()`: a program asked to stop leaves when it chooses to.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

fn program(name: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-stop-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, source).unwrap();
    file
}

fn kill(pid: u32, signal: &str) {
    let ok = Command::new("kill")
        .args([&format!("-{signal}"), &pid.to_string()])
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

#[test]
fn a_signal_only_sets_the_flag_once_the_program_asked() {
    let file = program(
        "clean",
        "Process.stopping()\nprint(\"ready\")\nlet turns = 0\nwhile not Process.stopping() do\n  time.sleep(20)\n  turns += 1\nend\nprint(\"left cleanly\")\n",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    kill(child.id(), "TERM");
    let mut rest = String::new();
    out.read_line(&mut rest).unwrap();
    assert_eq!(rest.trim(), "left cleanly");
    assert!(child.wait().unwrap().success());
}

#[test]
fn a_second_signal_ends_a_program_that_does_not_leave() {
    let file = program(
        "stubborn",
        "Process.stopping()\nprint(\"ready\")\nwhile true do\n  time.sleep(20)\nend\n",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    kill(child.id(), "TERM");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the first signal must not end it"
    );
    kill(child.id(), "TERM");
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
}

#[test]
fn without_a_call_the_signal_ends_the_program_as_before() {
    let file = program(
        "plain",
        "print(\"ready\")\nwhile true do\n  time.sleep(20)\nend\n",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    kill(child.id(), "TERM");
    assert!(!child.wait().unwrap().success());
}
