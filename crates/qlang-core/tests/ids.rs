//! `time`, `random`, `uuid` and `ulid`: the host provides the clock and the randomness.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn host(clock: Option<i64>, seed: Option<u64>) -> MemoryHost {
    MemoryHost {
        clock,
        seed,
        ..MemoryHost::default()
    }
}

/// Output of the program, or `CODE: message` (compile or run error).
fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return format!(
                        "COMPILE: {}",
                        e.diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.code, d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn ok(src: &str) -> String {
    exec(src, host(Some(1_700_000_000_000), Some(42)))
}

#[test]
fn time_now_asks_the_host() {
    assert_eq!(ok("print(time.now())"), "1700000000000\n");
}

#[test]
fn the_same_seed_gives_the_same_numbers() {
    let program = "print(random.int(1, 1000000))\nprint(random.float())";
    assert_eq!(
        exec(program, host(None, Some(7))),
        exec(program, host(None, Some(7)))
    );
    assert_ne!(
        exec(program, host(None, Some(7))),
        exec(program, host(None, Some(8)))
    );
}

#[test]
fn random_int_stays_in_range_and_includes_both_ends() {
    let out = ok(
        "let seen = [0, 0, 0]\nfor i in 0..300 do\n  seen[random.int(1, 3) - 1] = 1\nend\nprint(seen)\nprint(random.int(5, 5))",
    );
    assert_eq!(out, "[1, 1, 1]\n5\n");
    assert!(ok("print(random.int(3, 1))").starts_with("R062"));
    assert_eq!(
        ok("print(random.int(int.MIN, int.MAX) <= int.MAX)"),
        "true\n"
    );
}

#[test]
fn random_float_is_below_one() {
    assert_eq!(
        ok(
            "let ok = true\nfor i in 0..200 do\n  let x = random.float()\n  if x < 0.0 or x >= 1.0 then\n    ok = false\n  end\nend\nprint(ok)"
        ),
        "true\n"
    );
}

#[test]
fn uuid_formats_and_versions() {
    let out = ok(
        "let a = uuid.v4()\nprint(a.version())\nprint((a as string).len())\nprint(uuid.v7().version())\nprint(uuid.NIL)",
    );
    assert_eq!(out, "4\n36\n7\n00000000-0000-0000-0000-000000000000\n");
}

#[test]
fn uuid_v7_carries_the_time_and_sorts_by_it() {
    let out = ok("let a = uuid.v7()\nprint((a as string).substring(0, 13))");
    // 1_700_000_000_000 ms = 0x018b_cfe5_6800
    assert_eq!(out, "018bcfe5-6800\n");
}

#[test]
fn uuid_parse() {
    let out = ok(
        "let u = uuid.parse(\"550E8400-e29b-41d4-a716-446655440000\")\nif u != none then\nprint(u)\nend\nprint(uuid.parse(\"nope\") == none)",
    );
    assert_eq!(out, "550e8400-e29b-41d4-a716-446655440000\ntrue\n");
}

#[test]
fn ulid_new_has_the_time_first() {
    let out = ok("let id = ulid.new()\nprint((id as string).len())\nprint(id.time())");
    assert_eq!(out, "26\n1700000000000\n");
}

#[test]
fn ulid_made_in_the_same_millisecond_still_sort() {
    let out = ok(
        "let ids: array<ulid> = []\nfor i in 0..50 do\n  ids.push(ulid.new())\nend\nlet sorted = true\nfor i in 1..50 do\n  if ids[i] <= ids[i - 1] then\n    sorted = false\n  end\nend\nprint(sorted)",
    );
    assert_eq!(out, "true\n");
}

#[test]
fn ulid_at_and_parse() {
    let out = ok(
        "let id = ulid.at(1469918176385)\nprint((id as string).substring(0, 10))\nprint(id.time())\nlet p = ulid.parse(\"01ARYZ6S41TSV4RRFFQ69G5FAV\")\nif p != none then\nprint(p)\nend\nprint(ulid.parse(\"zz\") == none)",
    );
    assert_eq!(
        out,
        "01ARYZ6S41\n1469918176385\n01ARYZ6S41TSV4RRFFQ69G5FAV\ntrue\n"
    );
    assert!(ok("print(ulid.at(-1))").starts_with("R061"));
}

#[test]
fn ids_compare_sort_and_are_map_keys() {
    let out = ok(
        "let a = ulid.at(1000)\nlet b = ulid.at(2000)\nprint(a < b)\nprint(a == a)\nlet xs = [b, a]\nxs.sort()\nprint(xs[0] == a)\nlet m: map<ulid, string> = {}\nm[a] = \"first\"\nprint(m[a])\nlet u: uuid = uuid.NIL\nprint(u == uuid.NIL)",
    );
    assert_eq!(out, "true\ntrue\ntrue\nfirst\ntrue\n");
}

#[test]
fn ids_convert_to_string_in_text() {
    let out = ok("print(\"id: {uuid.NIL}\")\nlet xs = [uuid.NIL]\nprint(xs)");
    assert_eq!(
        out,
        "id: 00000000-0000-0000-0000-000000000000\n[00000000-0000-0000-0000-000000000000]\n"
    );
}

#[test]
fn types_do_not_mix() {
    assert!(ok("let u: uuid = ulid.at(1)").starts_with("COMPILE"));
    assert!(ok("let s: string = uuid.NIL").starts_with("COMPILE"));
    assert!(ok("print(uuid.NIL == ulid.at(1))").starts_with("COMPILE"));
}

#[test]
fn no_clock_or_randomness_is_reported_before_running() {
    for src in [
        "print(time.now())",
        "print(random.int(1, 2))",
        "print(random.float())",
        "print(uuid.v4())",
        "print(uuid.v7())",
        "print(ulid.new())",
        "print(ulid.at(1))",
    ] {
        let out = exec(src, host(None, None));
        assert!(out.starts_with("COMPILE: T250"), "{src}: {out}");
    }
    // only one of the two is missing
    assert!(exec("print(uuid.v7())", host(Some(1), None)).starts_with("COMPILE: T250"));
    assert!(exec("print(uuid.v4())", host(None, Some(1))).len() > 30);
    assert!(exec("print(ulid.new())", host(None, Some(1))).starts_with("COMPILE: T250"));
    // what does not need them still works
    assert_eq!(exec("print(uuid.NIL.version())", host(None, None)), "0\n");
}

#[test]
fn a_variable_named_like_a_namespace_wins() {
    assert_eq!(
        exec(
            "let time = 5\nlet random = time + 1\nprint(random)",
            host(None, None)
        ),
        "6\n"
    );
}

#[test]
fn the_json_protocol_carries_the_clock_and_the_seed() {
    use qlang_core::protocol::{Ceiling, handle};
    let ask = |extra: &str| {
        handle(
            &format!(
                r#"{{"files":{{"main.q":"print(time.now())\nprint(random.int(1, 100))"}}{extra}}}"#
            ),
            &Ceiling::default(),
        )
    };
    let with = ask(r#","now":1234,"seed":9"#);
    assert!(with.contains(r#""output":"1234\n"#), "{with}");
    assert_eq!(with, ask(r#","now":1234,"seed":9"#));
    let without = ask("");
    assert!(without.contains("T250"), "{without}");
}
