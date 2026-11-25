//! `qlang check <dir>` and `qlang test --check-all`.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-check-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files = [
        ("a.q", "fun a() -> int\n  1\nend\nexport a\n"),
        ("b.q", "fun b() -> int\n  \"x\"\nend\nexport b\n"),
        ("c.q", "import b from \"./b.q\"\nprint(b())\n"),
        ("d.q", "import b from \"./b.q\"\nprint(b())\n"),
        (
            "a_test.q",
            "import a from \"./a.q\"\nfun test_a()\n  assert_eq(a(), 1)\nend\n",
        ),
    ];
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn qlang(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stderr).into_owned() + &String::from_utf8_lossy(&o.stdout),
        o.status.code().unwrap_or(-1),
    )
}

#[test]
fn check_takes_a_directory_and_prints_a_shared_mistake_once() {
    let dir = project("dir");
    let (out, code) = qlang(&dir, &["check", "."]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out.matches("T002").count(), 1, "{out}");
}

#[test]
fn a_test_run_can_check_every_file_first() {
    let dir = project("all");
    let (out, code) = qlang(&dir, &["test"]);
    assert_eq!(code, 0, "{out}");
    let (out, code) = qlang(&dir, &["test", "--check-all"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("T002"), "{out}");
    assert!(out.contains("tests were not run"), "{out}");
}
