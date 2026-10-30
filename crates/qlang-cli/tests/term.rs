//! `Term` when the program is not run from a terminal (a pipe, a file): the size and the
//! colours are answered, the rest says that a terminal is needed.

use std::process::{Command, Stdio};

fn run(source: &str, env: &[(&str, &str)]) -> (String, String) {
    // one folder for each test: they run at the same time
    let name = std::thread::current().name().unwrap_or("t").to_string();
    let dir = std::env::temp_dir().join(format!("qlang-term-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.q"), source).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_qlang"));
    cmd.arg("run")
        .arg("main.q")
        .current_dir(&dir)
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let o = cmd.output().unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn outside_a_terminal_the_size_comes_from_the_environment_and_there_is_no_colour() {
    let src = "let s = Term.size()\nprint(\"{Term.is_tty()} {s.cols}x{s.rows} {Term.colors()}\")";
    let (out, _) = run(src, &[("COLUMNS", "120"), ("LINES", "40")]);
    assert_eq!(out.trim(), "false 120x40 0");
    let (out, _) = run(src, &[("COLUMNS", "x")]);
    assert_eq!(out.trim(), "false 80x24 0");
}

#[test]
fn outside_a_terminal_raw_mode_and_events_are_errors_not_crashes() {
    let src = "print(Term.raw(true).unwrap_err().message)\nprint(Term.event(10).unwrap_err().message)\nprint(Term.mouse(true).is_err())";
    let (out, err) = run(src, &[]);
    assert!(out.contains("needs a terminal"), "{out} {err}");
    assert!(out.contains("true"), "{out}");
}

#[test]
fn write_sends_the_text_as_it_is_without_a_newline() {
    let (out, _) = run("Term.write(\"\\e[1mhi\\e[0m\")", &[]);
    assert_eq!(out, "\u{1b}[1mhi\u{1b}[0m");
}

#[test]
fn a_session_outside_a_terminal_gives_the_error_and_does_not_run_the_body() {
    let src =
        "let r = Term.session(fun() -> bool\n  print(\"body\")\n  true\nend)\nprint(r.is_err())";
    let (out, _) = run(src, &[]);
    assert_eq!(out.trim(), "true");
}
