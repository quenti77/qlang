//! The assertion functions of the prelude, and where their failures are reported.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// Output of the program, or `CODE: message | rendered error` (compile or run error).
fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => return format!("COMPILE: {}", e.render()),
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => d.render(&prog.sources),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

const FAILS: &str = "fun fails() -> Result<int, Error>\n  Err(Error { message: \"nope\" })\nend\nfun works() -> Result<int, Error>\n  Ok(7)\nend\n";

#[test]
fn ok_and_err_give_back_the_value_to_go_on_with() {
    let out = exec(&format!(
        "{FAILS}print(assert_ok(works()) + 1)\nprint(assert_err(fails()).message)"
    ));
    assert_eq!(out, "8\nnope\n");
}

#[test]
fn a_wrong_result_fails_showing_what_it_held() {
    let out = exec(&format!("{FAILS}assert_ok(fails())"));
    assert!(
        out.contains("assert_ok failed: got the error Error { message: \"nope\" }"),
        "{out}"
    );
    let out = exec(&format!("{FAILS}assert_err(works())"));
    assert!(
        out.contains("assert_err failed: got the ok value 7"),
        "{out}"
    );
}

#[test]
fn some_and_none() {
    assert_eq!(
        exec("print(assert_some(5) + 1)\nassert_none(none)\nlet n: int? = none\nassert_none(n)"),
        "6\n"
    );
    let out = exec("let n: int? = none\nassert_some(n)");
    assert!(
        out.contains("assert_some failed: the value is none"),
        "{out}"
    );
    let out = exec("let n: int? = 3\nassert_none(n)");
    assert!(out.contains("assert_none failed: got 3"), "{out}");
}

#[test]
fn contains_for_arrays_and_text() {
    assert_eq!(
        exec(
            "assert_contains([1, 2, 3], 2)\nassert_contains([\"a\"], \"a\")\nassert_contains_text(\"hello world\", \"o w\")\nprint(\"fine\")"
        ),
        "fine\n"
    );
    let out = exec("assert_contains([1, 2], 9)");
    assert!(out.contains("[1, 2] does not contain 9"), "{out}");
    let out = exec("assert_contains_text(\"abc\", \"x\")");
    assert!(out.contains("\"abc\" does not contain \"x\""), "{out}");
}

#[test]
fn close_and_within_forgive_float_noise() {
    assert_eq!(
        exec(
            "assert_close(0.1 + 0.2, 0.3)\nassert_within(100.5, 100.0, 0.01)\nassert_close(1.0, 1.0)\nprint(\"fine\")"
        ),
        "fine\n"
    );
    let out = exec("assert_close(0.3, 0.31)");
    assert!(
        out.contains("assert_within failed") && out.contains("0.31"),
        "{out}"
    );
    // the tolerance is relative once the expected value is large
    assert_eq!(
        exec("assert_within(1000000.5, 1000000.0, 0.000001)\nprint(\"fine\")"),
        "fine\n"
    );
    assert!(exec("assert_within(1.5, 1.0, 0.1)").contains("not within 0.1 of 1.0"));
}

#[test]
fn a_failure_is_shown_at_the_users_line_not_in_the_prelude() {
    let out = exec("let a = 1\nassert_eq(a, 2)\n");
    assert!(out.contains("main.q:2:1"), "{out}");
    assert!(out.contains("assert_eq(a, 2)"), "{out}");
    assert!(!out.contains("<prelude>"), "{out}");
    assert!(out.contains("raised inside `assert_eq`"), "{out}");
}

#[test]
fn errors_in_the_users_own_code_are_unchanged() {
    let out = exec("fun f(n: int) -> int\n  if n == 0 then panic(\"zero\") end\n  n\nend\nf(0)\n");
    assert!(out.contains("main.q:2:"), "{out}");
    assert!(!out.contains("raised inside"), "{out}");
}

#[test]
fn unwrap_on_an_error_points_at_the_call() {
    let out = exec(&format!("{FAILS}fails().unwrap()\n"));
    assert!(out.contains("main.q:7:1"), "{out}");
    assert!(!out.contains("<prelude>"), "{out}");
}

#[test]
fn comparisons_lengths_and_text_assertions() {
    let out = exec(
        "assert_lt(1, 2)\nassert_le(2, 2)\nassert_gt(\"b\", \"a\")\nassert_ge(2, 2)\nassert_lt(0.5, 1.5)\nassert_len([1, 2, 3], 3)\nassert_empty([])\nassert_not_contains([1, 2], 3)\nassert_starts_with(\"hello\", \"he\")\nassert_ends_with(\"hello\", \"llo\")\nprint(\"ok\")",
    );
    assert_eq!(out, "ok\n");
    for (src, needle) in [
        ("assert_lt(3, 2)", "assert_lt failed: 3 is not less than 2"),
        ("assert_le(3, 2)", "assert_le failed"),
        (
            "assert_gt(1, 2)",
            "assert_gt failed: 1 is not greater than 2",
        ),
        ("assert_ge(1, 2)", "assert_ge failed"),
        (
            "assert_len([1], 2)",
            "assert_len failed: [1] has 1 element(s), expected 2",
        ),
        ("assert_empty([1])", "assert_empty failed: got [1]"),
        ("assert_not_contains([1], 1)", "assert_not_contains failed"),
        ("assert_starts_with(\"abc\", \"b\")", "does not start with"),
        ("assert_ends_with(\"abc\", \"b\")", "does not end with"),
    ] {
        let out = exec(src);
        assert!(out.contains(needle), "{src}: {out}");
        // the failure points at the test, not at the prelude
        assert!(out.contains("main.q"), "{src}: {out}");
    }
}
