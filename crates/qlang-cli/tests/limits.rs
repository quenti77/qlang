//! The memory limit of the real `qlang` program (`R904`).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn run(args: &[&str], source: &str) -> (String, String, Option<i32>) {
    let dir = std::env::temp_dir().join(format!("qlang-limits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("p{}.q", COUNTER.fetch_add(1, Ordering::SeqCst)));
    std::fs::write(&file, source).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .args(args)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

const GREEDY: &str = "let keep: array<array<int>> = []\nfor i in 0..100 do\n  keep.push([0].repeat(5000000))\nend\nprint(keep.len())\n";

#[test]
fn a_program_holding_too_much_memory_is_stopped() {
    let (_, err, code) = run(&["--max-memory", "200"], GREEDY);
    assert!(err.contains("R904"), "{err}");
    assert_eq!(code, Some(2));
}

#[test]
fn zero_removes_the_limit_and_a_small_program_is_unaffected() {
    let (out, _, code) = run(&["--max-memory", "0"], "print(1 + 1)\n");
    assert_eq!((out.as_str(), code), ("2\n", Some(0)));
    let (out, _, _) = run(
        &["--max-memory", "200"],
        "let a = [1, 2, 3]\nprint(a.len())\n",
    );
    assert_eq!(out, "3\n");
}

#[test]
fn the_option_needs_a_number() {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "--max-memory", "lots"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(64));
}
