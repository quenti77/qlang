//! `qlang fmt` on files.

use std::process::Command;

fn qlang() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qlang"))
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("qlang-fmt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn fmt_rewrites_files_and_check_lists_them_without_touching_them() {
    let d = dir("write");
    let file = d.join("a.q");
    let messy = "fun f()\nlet a = 1   \nend\n";
    std::fs::write(&file, messy).unwrap();

    let out = qlang()
        .args(["fmt", "--check"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("a.q"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), messy);

    let out = qlang().arg("fmt").arg(&file).output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "fun f()\n    let a = 1\nend\n"
    );

    // now it is laid out: --check is quiet and succeeds
    let out = qlang()
        .args(["fmt", "--check"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn a_file_that_does_not_lex_is_reported_and_left_alone() {
    let d = dir("broken");
    let file = d.join("b.q");
    std::fs::write(&file, "let s = \"open\n").unwrap();
    let out = qlang().arg("fmt").arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(65));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "let s = \"open\n");
}

#[test]
fn a_folder_is_searched_for_q_files() {
    let d = dir("folder");
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub/c.q"), "if true then\nprint(1)\nend\n").unwrap();
    std::fs::write(d.join("notes.txt"), "not code").unwrap();
    let out = qlang().args(["fmt", "--check"]).arg(&d).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("c.q"));
}

#[test]
fn diff_shows_a_unified_diff_and_check_with_diff_leaves_the_file_alone() {
    let d = dir("diff");
    let file = d.join("a.q");
    let messy = "fun f()\nlet a = 1   \nend\n";
    std::fs::write(&file, messy).unwrap();

    let out = qlang()
        .args(["fmt", "--check", "--diff"])
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("--- ") && text.contains("a.q (formatted)"),
        "{text}"
    );
    assert!(text.contains("@@"), "{text}");
    assert!(text.contains("-let a = 1   \n"), "{text}");
    assert!(text.contains("+    let a = 1\n"), "{text}");
    assert!(
        !text.contains('\x1b'),
        "no colour outside a terminal: {text}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), messy);

    // without --check the file is written, and the diff says what changed
    let out = qlang().args(["fmt", "--diff"]).arg(&file).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("+    let a = 1"));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "fun f()\n    let a = 1\nend\n"
    );

    // nothing to change: no output
    let out = qlang()
        .args(["fmt", "--check", "--diff"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn diff_on_the_standard_input() {
    use std::io::Write;
    let mut child = qlang()
        .args(["fmt", "--stdin", "--diff"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fun f()\nlet a = 1\nend\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("<stdin>") && text.contains("+    let a = 1"),
        "{text}"
    );
}
