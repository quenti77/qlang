//! `exit`, `read_all` and `time.sleep`.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

struct Outcome {
    output: String,
    error: Option<String>,
    exit_code: Option<i32>,
}

fn run_with(src: &str, mut host: MemoryHost) -> Outcome {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return Outcome {
                        output: String::new(),
                        error: Some(format!(
                            "COMPILE: {}",
                            e.diagnostics
                                .iter()
                                .map(|d| format!("{}: {}", d.code, d.message))
                                .collect::<Vec<_>>()
                                .join("; ")
                        )),
                        exit_code: None,
                    };
                }
            };
            let r = prog.run(&mut host, Limits::default());
            Outcome {
                output: host.output,
                error: r.error.map(|d| format!("{}: {}", d.code, d.message)),
                exit_code: r.exit_code,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn run(src: &str) -> Outcome {
    run_with(src, MemoryHost::default())
}

#[test]
fn exit_ends_the_program_with_its_code() {
    let o = run("print(\"before\")\nexit(3)\nprint(\"after\")");
    assert_eq!(o.output, "before\n");
    assert_eq!(o.error, None);
    assert_eq!(o.exit_code, Some(3));
}

#[test]
fn exit_works_from_inside_functions_and_loops() {
    let o = run(
        "fun check(n: int)\n  if n == 3 then\n    print(\"stop\")\n    exit(7)\n  end\nend\nfor i in 0..10 do\n  check(i)\n  print(i)\n  end\nprint(\"never\")",
    );
    assert_eq!(o.output, "0\n1\n2\nstop\n");
    assert_eq!(o.exit_code, Some(7));
}

#[test]
fn exit_without_a_code_is_zero_and_a_branch_may_end_with_it() {
    let o = run(
        "fun f(x: int) -> int\n  if x > 0 then return x end\n  exit()\nend\nprint(f(2))\nprint(f(0))",
    );
    assert_eq!(o.output, "2\n");
    assert_eq!(o.exit_code, Some(0));
    assert!(run("exit(\"a\")").error.unwrap().starts_with("COMPILE:"));
    assert!(run("exit(1, 2)").error.unwrap().starts_with("COMPILE:"));
}

#[test]
fn a_program_that_does_not_call_exit_has_no_code() {
    assert_eq!(run("print(1)").exit_code, None);
}

#[test]
fn read_all_returns_the_remaining_lines() {
    let host = MemoryHost::with_input(&["first", "second", "third"]);
    let o = run_with(
        "let a = read()\nlet rest = read_all()\nprint(a)\nprint(rest.lines().len())\nprint(rest)\nprint(read_all().len())",
        host,
    );
    assert_eq!(o.error, None);
    assert_eq!(o.output, "first\n2\nsecond\nthird\n0\n");
}

#[test]
fn read_all_needs_input_like_read() {
    let o = run("print(read_all())");
    assert!(o.error.unwrap().contains("T250"));
}

#[test]
fn sleep_moves_a_virtual_clock_forward() {
    let host = MemoryHost {
        clock: Some(1_000),
        ..MemoryHost::default()
    };
    let o = run_with(
        "let a = time.now()\ntime.sleep(250)\nlet b = time.now()\nprint(b - a)\ntime.sleep(0)\nprint(time.now() - a)",
        host,
    );
    assert_eq!(o.error, None);
    assert_eq!(o.output, "250\n250\n");
}

#[test]
fn sleep_needs_a_clock_and_a_sane_duration() {
    assert!(run("time.sleep(10)").error.unwrap().contains("T250"));
    let clock = || MemoryHost {
        clock: Some(0),
        ..MemoryHost::default()
    };
    assert!(
        run_with("time.sleep(-1)", clock())
            .error
            .unwrap()
            .contains("R017")
    );
    assert!(
        run_with("time.sleep(99999999)", clock())
            .error
            .unwrap()
            .contains("R017")
    );
}
