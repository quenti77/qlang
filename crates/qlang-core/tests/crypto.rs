//! `Crypto`: secure random bytes, tokens and a comparison that does not leak.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(host: &mut MemoryHost, src: &str) -> String {
    match compile("main.q", src, host) {
        Ok(p) => {
            let r = p.run(host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            codes.join(",")
        }
    }
}

#[test]
fn tokens_are_url_safe_and_the_size_asked() {
    let mut host = MemoryHost {
        seed: Some(3),
        ..MemoryHost::default()
    };
    let out = run(
        &mut host,
        "let a = Crypto.token(32).unwrap()
let b = Crypto.token(32).unwrap()
print(a.len())
print(a == b)
print(a.contains(\"+\") or a.contains(\"/\") or a.contains(\"=\"))
print(Crypto.random_bytes(0).unwrap().len())
print(Crypto.random_bytes(2000000).is_err())
",
    );
    assert_eq!(out, "43\nfalse\nfalse\n0\ntrue\n");
}

#[test]
fn equal_compares_whole_texts() {
    let mut host = MemoryHost::default();
    let out = run(
        &mut host,
        "print(Crypto.equal(\"abc\", \"abc\"))
print(Crypto.equal(\"abc\", \"abd\"))
print(Crypto.equal(\"abc\", \"abcd\"))
print(Crypto.equal(\"\", \"\"))
print(Crypto.equal_bytes(\"a\".to_bytes(), \"a\".to_bytes()))
",
    );
    assert_eq!(out, "true\nfalse\nfalse\ntrue\ntrue\n");
}

#[test]
fn a_host_without_randomness_says_so_before_running() {
    let mut host = MemoryHost::default();
    assert_eq!(run(&mut host, "print(Crypto.token(8).unwrap())\n"), "T250");
}

#[test]
fn password_hashing_is_only_offered_by_the_command() {
    let mut host = MemoryHost::default();
    assert_eq!(
        run(&mut host, "print(Password.hash(\"x\").is_ok())\n"),
        "T250"
    );
}

#[test]
fn the_log_goes_to_the_host_with_its_fields() {
    let mut host = MemoryHost::default();
    let out = run(
        &mut host,
        "Log.info(\"started\")
Log.error(\"boom\")
Log.write(\"warn\", \"slow\", { \"ms\": \"350\" })
Log.write(\"loud\", \"odd level\", {})
",
    );
    assert_eq!(out, "");
    assert_eq!(
        host.logs,
        [
            "info started",
            "error boom",
            "warn slow ms=350",
            "info odd level"
        ]
    );
}
