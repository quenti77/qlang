//! `Process.run`, through the real `qlang` program (unix: it starts `echo`, `cat` and `sh`).
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-process-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        fs::write(self.0.join(rel), text).unwrap();
    }

    fn qlang(&self, args: &[&str], stdin: &str) -> (String, String, Option<i32>) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(&self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let o = child.wait_with_output().unwrap();
        (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
            o.status.code(),
        )
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const PROGRAM: &str = "let r = assert_ok(Process.run(\"echo\", [\"hello\", \"world\"]))\nprint(r.status)\nprint(r.stdout.trim())\nlet c = assert_ok(Process.run_with_input(\"cat\", [], \"from stdin\"))\nprint(c.stdout)\nlet f = assert_ok(Process.run(\"sh\", [\"-c\", \"echo oops >&2; exit 3\"]))\nprint(f.status)\nprint(f.stderr.trim())\nprint(Process.run(\"no-such-program-xyz\", []).is_err())\n";

#[test]
fn programs_cannot_be_started_without_the_flag() {
    let d = Dir::new("no-flag");
    d.write("a.q", PROGRAM);
    let (_, err, code) = d.qlang(&["run", "a.q"], "");
    assert_eq!(code, Some(1));
    assert!(
        err.contains("T250") && err.contains("--allow-process"),
        "{err}"
    );
    // a check only looks at the program: nothing is started, so it needs no permission
    assert_eq!(d.qlang(&["check", "a.q"], "").2, Some(0));
}

#[test]
fn with_the_flag_a_program_runs_and_its_output_comes_back() {
    let d = Dir::new("flag");
    d.write("a.q", PROGRAM);
    let (out, err, code) = d.qlang(&["run", "--allow-process", "a.q"], "");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "0\nhello world\nfrom stdin\n3\noops\ntrue\n");
}

#[test]
fn the_json_protocol_never_starts_programs() {
    let d = Dir::new("json");
    let request = serde_json::json!({
        "files": { "main.q": "print(Process.run(\"echo\", [\"x\"]).is_ok())" },
    });
    let (out, _, _) = d.qlang(&["run", "--json", "--allow-process"], &request.to_string());
    assert!(out.contains("T250"), "{out}");
}

#[test]
fn tests_may_start_programs_with_the_flag() {
    let d = Dir::new("test");
    d.write("p_test.q", "fun test_echo()\n  let r = assert_ok(Process.run(\"echo\", [\"hi\"]))\n  assert_eq(r.stdout.trim(), \"hi\")\nend\n");
    assert_eq!(d.qlang(&["test"], "").2, Some(1));
    let (out, _, code) = d.qlang(&["test", "--allow-process"], "");
    assert_eq!(code, Some(0), "{out}");
}
