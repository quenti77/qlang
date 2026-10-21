//! `exit`, `read_all` and `time.sleep` through the real `qlang` program.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-system-{}-{name}", std::process::id()));
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

#[test]
fn exit_gives_the_process_its_code() {
    let d = Dir::new("exit");
    d.write("a.q", "print(\"bye\")\nexit(5)\nprint(\"never\")\n");
    let (out, _, code) = d.qlang(&["run", "a.q"], "");
    assert_eq!(out, "bye\n");
    assert_eq!(code, Some(5));
    d.write("ok.q", "exit()\n");
    assert_eq!(d.qlang(&["run", "ok.q"], "").2, Some(0));
}

#[test]
fn read_all_reads_the_standard_input() {
    let d = Dir::new("read-all");
    d.write(
        "a.q",
        "let text = read_all()\nprint(text.lines().len())\nprint(text.upper())\n",
    );
    let (out, err, code) = d.qlang(&["run", "a.q"], "one\ntwo\nthree\n");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "3\nONE\nTWO\nTHREE\n");
}

#[test]
fn sleep_really_waits() {
    let d = Dir::new("sleep");
    d.write(
        "a.q",
        "let a = time.now()\ntime.sleep(120)\nprint(time.now() - a >= 120)\n",
    );
    let (out, err, code) = d.qlang(&["run", "a.q"], "");
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "true\n");
}

#[test]
fn a_test_may_exit_with_zero_but_not_with_another_code() {
    let d = Dir::new("test-exit");
    d.write(
        "a_test.q",
        "fun test_zero()\n  exit(0)\nend\nfun test_one()\n  exit(1)\nend\n",
    );
    let (out, _, code) = d.qlang(&["test"], "");
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("ok   a_test.q::test_zero"), "{out}");
    assert!(
        out.contains("FAIL a_test.q::test_one") && out.contains("exit(1)"),
        "{out}"
    );
}

#[cfg(unix)]
#[test]
fn fs_write_through_a_link_cannot_leave_root() {
    let d = Dir::new("fs-link");
    let outside = std::env::temp_dir().join(format!("qlang-outside-{}.txt", std::process::id()));
    fs::write(&outside, "original").unwrap();
    fs::create_dir_all(d.0.join("jail")).unwrap();
    std::os::unix::fs::symlink(&outside, d.0.join("jail/link.txt")).unwrap();
    d.write(
        "jail/a.q",
        "let r = fs.write(\"link.txt\", \"pwned\")\nprint(r.is_err())\n",
    );
    let (out, err, _) = d.qlang(&["run", "--root", "jail", "jail/a.q"], "");
    assert_eq!(out, "true\n", "{err}");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "original");
    let _ = fs::remove_file(&outside);
}

#[cfg(unix)]
#[test]
fn fs_read_refuses_what_is_not_a_regular_file() {
    let d = Dir::new("fs-dev");
    d.write("a.q", "let r = fs.read(\"/dev/zero\")\nprint(r.is_err())\n");
    let (out, err, _) = d.qlang(&["run", "a.q"], "");
    assert_eq!(out, "true\n", "{err}");
}
