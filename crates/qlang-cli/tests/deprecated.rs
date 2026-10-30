//! `#[deprecated("...")]`: a call to what carries it is a warning (`W001`), and `--deny-warnings`
//! turns the warnings into a failure.

use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-deprecated-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn qlang(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const OLD: &str = "#[deprecated(\"use fresh() instead\")]\nfun stale() -> int\n  1\nend\n\nfun fresh() -> int\n  2\nend\n\nexport stale, fresh\n";

#[test]
fn a_call_to_something_deprecated_warns_at_the_call_and_the_program_still_runs() {
    let dir = project(
        "warn",
        &[
            ("old.q", OLD),
            (
                "main.q",
                "import stale, fresh from \"old.q\"\nprint(stale() + fresh())\n",
            ),
        ],
    );
    let (code, out, err) = qlang(&dir, &["run", "main.q"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.trim(), "3");
    assert!(
        err.contains("warning[W001]: `stale` is deprecated: use fresh() instead"),
        "{err}"
    );
    assert!(err.contains("main.q:2"), "{err}");
}

#[test]
fn deny_warnings_fails_check_run_and_test() {
    let dir = project(
        "deny",
        &[
            ("old.q", OLD),
            ("main.q", "import stale from \"old.q\"\nprint(stale())\n"),
            (
                "main_test.q",
                "import stale from \"old.q\"\nfun test_it()\n  assert_eq(stale(), 1)\nend\n",
            ),
        ],
    );
    for args in [
        vec!["check", "main.q", "--deny-warnings"],
        vec!["run", "main.q", "--deny-warnings"],
        vec!["test", "main_test.q", "--deny-warnings"],
    ] {
        let (code, out, err) = qlang(&dir, &args);
        assert_eq!(code, 1, "{args:?}: {out} {err}");
    }
    // without it, all pass
    assert_eq!(qlang(&dir, &["check", "main.q"]).0, 0);
    assert_eq!(qlang(&dir, &["test", "main_test.q"]).0, 0);
}

#[test]
fn a_program_without_warnings_passes_deny_warnings() {
    let dir = project(
        "clean",
        &[
            ("old.q", OLD),
            ("main.q", "import fresh from \"old.q\"\nprint(fresh())\n"),
        ],
    );
    let (code, out, _) = qlang(&dir, &["run", "main.q", "--deny-warnings"]);
    assert_eq!((code, out.trim()), (0, "2"));
}

#[test]
fn the_module_that_declares_it_may_still_use_it_and_log_warn_is_the_first_case() {
    let dir = project(
        "own",
        &[(
            "main.q",
            "#[deprecated(\"old\")]\nfun a() -> int\n  1\nend\nprint(a())\nLog.warn(\"x\")\n",
        )],
    );
    let (code, _, err) = qlang(&dir, &["run", "main.q"]);
    assert_eq!(code, 0);
    assert!(!err.contains("`a` is deprecated"), "{err}");
    assert!(
        err.contains("`warn` is deprecated: use Log.warning"),
        "{err}"
    );
}

#[test]
fn deprecated_needs_one_text() {
    let dir = project(
        "bad",
        &[(
            "main.q",
            "#[deprecated]\nfun a() -> int\n  1\nend\nprint(a())\n",
        )],
    );
    let (code, _, err) = qlang(&dir, &["check", "main.q"]);
    assert_eq!(code, 1);
    assert!(err.contains("T292"), "{err}");
}
