//! `Password` through the real `qlang` program.

use std::process::Command;

fn run(src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("qlang-password-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

#[test]
fn a_password_is_hashed_checked_and_rehashed() {
    let (ok, out) = run("let h = Password.hash(\"s3cret\").unwrap()
print(h.starts_with(\"$argon2id$v=19$\"))
print(Password.verify(\"s3cret\", h).unwrap())
print(Password.verify(\"nope\", h).unwrap())
print(Password.needs_rehash(h).unwrap())
print(Password.verify(\"x\", \"junk\").is_err())
print(Crypto.token(16).unwrap().len())
");
    assert!(ok, "{out}");
    assert_eq!(out, "true\ntrue\nfalse\nfalse\ntrue\n22\n");
}

#[test]
fn the_log_is_written_on_the_error_output_from_a_level_up() {
    let dir = std::env::temp_dir().join(format!("qlang-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(
        &file,
        "Log.debug(\"d\")\nLog.info(\"i\")\nLog.warn(\"w\")\nprint(\"out\")\n",
    )
    .unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("run")
            .arg(&file)
            .args(extra)
            .output()
            .unwrap()
    };
    let o = run(&[]);
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert_eq!(String::from_utf8_lossy(&o.stdout), "out\n");
    assert!(err.contains("INFO  i") && err.contains("WARN  w"), "{err}");
    assert!(!err.contains("DEBUG"), "{err}");
    let o = run(&["--log-level", "warn", "--log-format", "json"]);
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(
        err.contains("\"level\":\"warn\"") && !err.contains("\"info\""),
        "{err}"
    );
    assert!(!run(&["--log-level", "loud"]).status.success());
}
