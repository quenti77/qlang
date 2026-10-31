//! `qlang lint`.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-lint-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn lint(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("lint")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

const LIB: &str = "fun used() -> int\n  1\nend\nfun dead() -> int\n  2\nend\n#[used]\nfun by_name() -> int\n  3\nend\nfun test_x()\n  assert_eq(used(), 1)\nend\nexport used\n";

#[test]
fn it_reports_unused_names_and_unreachable_code() {
    let main = "import sign, other from \"lib.q\"\nfun f(n: int) -> int\n  let spare = 5\n  let _kept = 3\n  if n > 0 then\n    return other()\n    print(\"never\")\n  end\n  n\nend\nprint(f(1))\n";
    let lib = "export sign, other\nfun sign() -> int\n  1\nend\nfun other() -> int\n  2\nend\n";
    let dir = project("basic", &[("main.q", main), ("lib.q", lib)]);
    let (out, code) = lint(&dir, &["main.q"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("main.q:1:8: warning[L003]: `sign` is imported but never used"),
        "{out}"
    );
    assert!(!out.contains("`other` is imported"), "{out}");
    assert!(
        out.contains("main.q:3:7: warning[L001]: `spare` is never used"),
        "{out}"
    );
    assert!(!out.contains("_kept"), "{out}");
    assert!(out.contains("main.q:7:5: warning[L004]"), "{out}");
}

#[test]
fn a_private_function_nobody_calls_is_dead_unless_it_is_a_test_or_annotated() {
    let dir = project("dead", &[("lib.q", LIB)]);
    let (out, code) = lint(&dir, &["lib.q"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("function `dead` is never used"), "{out}");
    assert!(!out.contains("by_name") && !out.contains("test_x"), "{out}");
}

#[test]
fn complexity_has_a_threshold() {
    let body: String = (0..6)
        .map(|i| format!("  if n == {i} then\n    return {i}\n  end\n"))
        .collect();
    let src = format!("fun big(n: int) -> int\n{body}  n\nend\nprint(big(1))\n");
    let dir = project("complex", &[("big.q", &src)]);
    let (out, code) = lint(&dir, &["big.q", "--max-complexity", "5"]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("warning[L005]: function `big` has 7 paths"),
        "{out}"
    );
    let (out, code) = lint(&dir, &["big.q"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn a_clean_file_passes_and_a_broken_one_fails() {
    let dir = project(
        "clean",
        &[("ok.q", "let x = 1\nprint(x)\n"), ("bad.q", "let = \n")],
    );
    assert_eq!(lint(&dir, &["ok.q"]).1, 0);
    assert_eq!(lint(&dir, &["bad.q"]).1, 65);
}
