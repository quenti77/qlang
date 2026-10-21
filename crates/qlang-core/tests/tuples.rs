//! Tuples: `(int, string)` types, `(1, "a")` values, `t.0` access.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
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

#[test]
fn a_tuple_holds_values_of_different_types() {
    let out = exec(
        "let p = (1, \"a\", true)\nprint(p)\nprint(p.0)\nprint(p.1)\nprint(p.2)\nlet q: (int, string, bool) = p\nprint(q.1.upper())\nprint(\"{p}\")",
    );
    assert_eq!(out, "(1, \"a\", true)\n1\na\ntrue\nA\n(1, \"a\", true)\n");
}

#[test]
fn functions_take_and_return_tuples() {
    let out = exec(
        "fun bounds(xs: array<int>) -> (int, int)\n  (xs[0], xs[xs.len() - 1])\nend\nfun swap<A, B>(p: (A, B)) -> (B, A)\n  (p.1, p.0)\nend\nlet b = bounds([3, 1, 2])\nprint(b)\nprint(swap(b))\nprint(swap((1, \"x\")))\nprint(bounds([1, 2, 3]).1)",
    );
    assert_eq!(out, "(3, 2)\n(2, 3)\n(\"x\", 1)\n3\n");
}

#[test]
fn nested_tuples_and_positions_after_a_dot() {
    let out = exec("let t = (1, (2.5, true))\nprint(t.1.0)\nprint(t.1.1)\nprint(((1, 2), 3).0.1)");
    assert_eq!(out, "2.5\ntrue\n2\n");
}

#[test]
fn equality_compares_every_element() {
    let out = exec(
        "print((1, \"a\") == (1, \"a\"))\nprint((1, \"a\") == (2, \"a\"))\nprint((1, \"a\") != (1, \"b\"))\nprint(((1, 2), 3) == ((1, 2), 3))",
    );
    assert_eq!(out, "true\nfalse\ntrue\ntrue\n");
    assert!(exec("print((1, 2) == (1, \"a\"))").starts_with("COMPILE:"));
}

#[test]
fn tuples_are_map_keys_and_set_values() {
    let out = exec(
        "let grid = { (0, 0): \"origin\", (1, 2): \"p\" }\nprint(grid[(1, 2)])\nprint(grid[(9, 9)])\ngrid[(5, 5)] = \"q\"\nprint(grid.len())\nlet seen = Set.of([(0, 0), (1, 0), (0, 0)])\nprint(seen)\nprint(seen.contains((1, 0)))\nprint(seen.contains((2, 0)))",
    );
    assert_eq!(out, "p\nnone\n3\nSet((0, 0), (1, 0))\ntrue\nfalse\n");
    let bad = exec("let m: map<(int, array<int>), int> = {}");
    assert!(bad.contains("T270"), "{bad}");
}

#[test]
fn tuples_in_arrays_loops_and_nullables() {
    let out = exec(
        "let pairs: array<(string, int)> = [(\"x\", 1), (\"y\", 2)]\nlet total = 0\nfor p in pairs do\n  total += p.1\n  write(\"{p.0} \")\nend\nprint(total)\nlet maybe: (int, string)? = none\nprint(maybe == none)\nlet some: (int, string)? = (1, \"a\")\nif some != none then\n  print(some.0)\nend",
    );
    assert_eq!(out, "x y 3\ntrue\n1\n");
    assert!(exec("let m: (int, string)? = none\nprint(m.0)").contains("T162"));
}

#[test]
fn an_element_may_be_widened_in_a_declared_type() {
    let out = exec(
        "let t: (int?, string) = (5, \"a\")\nprint(t.0)\nlet u: (int?, string) = (none, \"b\")\nprint(u.0)",
    );
    assert_eq!(out, "5\nnone\n");
    assert!(exec("let t: (int, string) = (1, 2)").starts_with("COMPILE:"));
    assert!(exec("let t: (int, string) = (1, \"a\", 3)").starts_with("COMPILE:"));
}

#[test]
fn bad_uses_are_reported() {
    assert!(exec("let t = (1, 2)\nprint(t.2)").contains("T286"));
    assert!(exec("let a = 1\nprint(a.0)").contains("T287"));
    assert!(exec("let t = (1, 2)\nt.0 = 5").contains("a tuple cannot be changed"));
    assert!(exec("let z = (1,)").contains("P060"));
    assert!(exec("let t: (int,) = 1").contains("P060"));
    // a parenthesised type or expression is still just that
    assert_eq!(exec("let w: (int) = 5\nprint((w + 1) * 2)"), "12\n");
}
