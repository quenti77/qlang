//! `qlang test`: the `fun test_...()` of `*_test.q` and `test_*.q` files.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
    }

    fn qlang(&self, args: &[&str]) -> (String, String, Option<i32>) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
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

const PASSING: &str = "fun double(x: int) -> int\n  x * 2\nend\n\nfun test_double()\n  assert_eq(double(21), 42)\nend\n\nfun test_strings()\n  assert_eq(\"a\" + \"b\", \"ab\")\n  assert_ne(1, 2)\nend\n\nfun not_a_test()\n  panic(\"never called\")\nend\n";

#[test]
fn passing_tests_exit_with_zero() {
    let d = Dir::new("pass");
    d.write("qlang.toml", "[package]\nname = \"t\"\n");
    d.write("math_test.q", PASSING);
    let (out, err, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(out.contains("ok   math_test.q::test_double"), "{out}");
    assert!(out.contains("ok   math_test.q::test_strings"), "{out}");
    assert!(out.contains("2 passed, 0 failed"), "{out}");
    assert!(!out.contains("not_a_test"), "{out}");
}

#[test]
fn a_failing_test_is_shown_and_exits_with_one() {
    let d = Dir::new("fail");
    d.write(
        "a_test.q",
        "fun test_ok()\n  assert(true)\nend\nfun test_bad()\n  assert_eq([1, 2], [1, 3])\nend\n",
    );
    let (out, _, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("ok   a_test.q::test_ok"), "{out}");
    assert!(out.contains("FAIL a_test.q::test_bad"), "{out}");
    assert!(
        out.contains("actual:   [1, 2]") && out.contains("expected: [1, 3]"),
        "{out}"
    );
    assert!(out.contains("1 passed, 1 failed"), "{out}");
}

#[test]
fn each_test_starts_from_a_clean_state() {
    let d = Dir::new("clean");
    d.write(
        "state_test.q",
        "let counter = [0]\nfun test_a()\n  counter[] = 1\n  assert_eq(counter.len(), 2)\nend\nfun test_b()\n  counter[] = 1\n  assert_eq(counter.len(), 2)\nend\n",
    );
    let (out, _, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(0), "{out}");
}

#[test]
fn test_files_are_found_below_the_project_and_filtered() {
    let d = Dir::new("find");
    d.write("qlang.toml", "[package]\nname = \"t\"\n");
    d.write("src/deep/thing_test.q", PASSING);
    d.write("test_other.q", "fun test_x()\n  assert(true)\nend\n");
    d.write(
        ".qlang/packages/x/y_test.q",
        "fun test_hidden()\n  assert(false)\nend\n",
    );
    d.write(
        "lib.q",
        "fun test_not_a_test_file()\n  assert(false)\nend\n",
    );
    let (out, _, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("deep") && out.contains("test_other.q::test_x"),
        "{out}"
    );
    assert!(
        !out.contains("hidden") && !out.contains("not_a_test_file"),
        "{out}"
    );
    let (out, _, _) = d.qlang(&["test", "--filter", "strings"]);
    assert!(
        out.contains("test_strings") && !out.contains("test_double"),
        "{out}"
    );
}

#[test]
fn a_test_file_that_does_not_compile_fails_the_run() {
    let d = Dir::new("broken");
    d.write("a_test.q", "fun test_x()\n  let n: int = \"no\"\nend\n");
    let (out, _, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(1), "{out}");
    assert!(out.contains("it does not compile"), "{out}");
}

#[test]
fn no_tests_is_not_an_error() {
    let d = Dir::new("empty");
    let (out, _, code) = d.qlang(&["test"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("no tests found"), "{out}");
}
