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
        "fun f()\n  let a = 1\nend\n"
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
