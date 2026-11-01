use std::process::Command;

fn qlang(args: &[&str]) -> (String, String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(args)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

#[test]
fn every_command_answers_help_and_h_with_its_own_part_of_the_usage() {
    for command in commands() {
        for flag in ["--help", "-h"] {
            let (out, err, code) = qlang(&[command, flag]);
            assert_eq!(code, Some(0), "{command} {flag}: {err}");
            assert!(
                out.contains(&format!("qlang {command}")),
                "{command} {flag}: {out}"
            );
            assert!(out.contains("--max-memory"), "{command} {flag}: {out}");
            assert!(err.is_empty(), "{command} {flag}: {err}");
        }
    }
}

#[test]
fn the_help_of_a_command_leaves_the_others_out() {
    let (out, _, _) = qlang(&["run", "--help"]);
    assert!(out.contains("--allow-process"));
    assert!(!out.contains("qlang serve"));
    assert!(!out.contains("qlang fmt"));
}

#[test]
fn help_names_a_command_or_lists_them_all() {
    let (out, _, code) = qlang(&["help", "check"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("qlang check [file.q]"));
    let (all, _, code) = qlang(&["help"]);
    assert_eq!(code, Some(0));
    assert!(all.contains("qlang serve") && all.contains("qlang fmt"));
    let (_, err, code) = qlang(&["help", "nope"]);
    assert_eq!(code, Some(64));
    assert!(err.contains("no command `nope`"));
}

#[test]
fn what_follows_a_double_dash_belongs_to_the_program() {
    let dir = std::env::temp_dir().join(format!("qlang-help-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.q");
    std::fs::write(&file, "print(env.args())\n").unwrap();
    let (out, err, code) = qlang(&["run", file.to_str().unwrap(), "--", "-h", "--help"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out.trim(), "[\"-h\", \"--help\"]");
}

fn commands() -> [&'static str; 22] {
    [
        "init",
        "doc",
        "migrate",
        "info",
        "publish",
        "verify",
        "lint",
        "dap",
        "repl",
        "fmt",
        "test",
        "run",
        "check",
        "serve",
        "lsp",
        "self-update",
        "packages",
        "install",
        "update",
        "add",
        "search",
        "remove",
    ]
}
