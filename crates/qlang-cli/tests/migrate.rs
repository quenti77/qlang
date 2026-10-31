//! `qlang migrate`.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-migrate-{}-{name}", std::process::id()));
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn migrate(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("migrate")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

const OLD: &str = "Log.warn(\"careful\")\n-- Log.warn in a comment stays\nlet s = \"Log.warn in a string stays\"\nLog.info(\"fine\")\nfun f()\n  Log.warn(\"again\")\nend\n";

#[test]
fn it_renames_only_the_names_of_the_code() {
    let dir = project(
        "rename",
        &[
            ("a.q", OLD),
            ("sub/b.q", "Log.warn(\"x\")\n"),
            ("clean.q", "print(1)\n"),
        ],
    );
    let (out, code) = migrate(&dir, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("a.q: 2 change(s)") && out.contains("b.q: 1 change(s)"),
        "{out}"
    );
    assert!(!out.contains("clean.q"), "{out}");
    let a = std::fs::read_to_string(dir.join("a.q")).unwrap();
    assert_eq!(
        a,
        "Log.warning(\"careful\")\n-- Log.warn in a comment stays\nlet s = \"Log.warn in a string stays\"\nLog.info(\"fine\")\nfun f()\n  Log.warning(\"again\")\nend\n"
    );
    // a second run has nothing left to do
    assert_eq!(migrate(&dir, &["--check"]).1, 0);
}

#[test]
fn check_lists_without_touching() {
    let dir = project("check", &[("a.q", OLD)]);
    let (out, code) = migrate(&dir, &["--check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("a.q: 2 change(s)"), "{out}");
    assert_eq!(std::fs::read_to_string(dir.join("a.q")).unwrap(), OLD);
}

#[test]
fn rules_are_listed() {
    let dir = project("rules", &[("a.q", "print(1)\n")]);
    let (out, code) = migrate(&dir, &["--rules"]);
    assert_eq!(code, 0);
    assert!(out.contains("0.69: Log.warn -> Log.warning"), "{out}");
}
