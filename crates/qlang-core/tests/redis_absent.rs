//! `Redis` is offered by the `qlang` command only: other hosts say so, and a program that does not
//! use it is not affected.

use qlang_core::check::compile;
use qlang_core::host::MemoryHost;

fn codes(src: &str) -> Vec<String> {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(_) => Vec::new(),
        Err(e) => e.diagnostics.iter().map(|d| d.code.to_string()).collect(),
    }
}

#[test]
fn a_host_without_redis_refuses_it_where_it_is_used() {
    assert_eq!(
        codes("let r = Redis.connect(\"redis://x\")\n"),
        vec!["T250"]
    );
    assert_eq!(
        codes("let s = Redis.subscriber(\"redis://x\", [\"a\"])\n"),
        vec!["T250"]
    );
}

#[test]
fn a_program_that_never_uses_redis_compiles_anywhere() {
    assert!(codes("print(1)\n").is_empty());
}

#[test]
fn signatures_and_stopping_are_for_the_qlang_command_only() {
    assert_eq!(
        codes("let r = Crypto.verify(\"RS256\", \"k\", \"m\".to_bytes(), \"s\".to_bytes())\n"),
        vec!["T250"]
    );
    assert_eq!(
        codes("let r = Crypto.sign(\"RS256\", \"k\", \"m\".to_bytes())\n"),
        vec!["T250"]
    );
    assert_eq!(codes("let r = Process.stopping()\n"), vec!["T250"]);
}
