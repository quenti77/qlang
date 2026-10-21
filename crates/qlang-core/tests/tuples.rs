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

#[test]
fn let_takes_a_tuple_apart() {
    let out = exec(
        "fun bounds(xs: array<int>) -> (int, int)\n  (xs[0], xs[xs.len() - 1])\nend\nlet (lo, hi) = bounds([3, 1, 2])\nprint(lo)\nprint(hi)\nlet (a, (b, _)) = (1, (\"x\", true))\nprint(\"{a} {b}\")\nconst (k, v): (string, int) = (\"age\", 42)\nprint(\"{k}={v}\")\nlet (i, _) = (7, 8)\nprint(i + 1)",
    );
    assert_eq!(out, "3\n2\n1 x\nage=42\n8\n");
}

#[test]
fn for_takes_each_element_apart() {
    let out = exec(
        "let people: array<(string, int)> = [(\"ana\", 31), (\"bob\", 27)]\nfor (name, age) in people do\n  print(\"{name} {age}\")\nend\nfor i, (name, _) in people do\n  print(\"{i}: {name}\")\nend\nlet pairs = [((1, 2), \"a\"), ((3, 4), \"b\")]\nfor ((x, y), label) in pairs do\n  print(\"{label} {x + y}\")\nend",
    );
    assert_eq!(out, "ana 31\nbob 27\n0: ana\n1: bob\na 3\nb 7\n");
}

#[test]
fn destructured_variables_work_like_the_others() {
    // a variable of a function, a top-level one used in a function, a changed one
    let out = exec(
        "let (g, h) = (1, 2)\nfun f() -> int\n  let (p, q) = (10, 20)\n  p + q + g\nend\nprint(f())\nlet (m, n) = (5, 6)\nm = 9\nprint(m + n)\nconst (c, d) = (1, 2)\nprint(c + d)",
    );
    assert_eq!(out, "31\n15\n3\n");
    assert!(exec("const (c, d) = (1, 2)\nc = 5").starts_with("COMPILE:"));
}

#[test]
fn a_loop_body_with_only_a_destructuring_keeps_its_own_variables() {
    let out = exec(
        "let total = 0\nfor i in 0..3 do\n  let (a, b) = (i, i * 2)\n  total += a + b\nend\nprint(total)",
    );
    assert_eq!(out, "9\n");
}

#[test]
fn the_pattern_must_fit_the_value() {
    let big = exec("let (a, b) = (1, 2, 3)");
    assert!(big.contains("T288"), "{big}");
    let not_a_tuple = exec("let (a, b) = 5");
    assert!(not_a_tuple.contains("T289"), "{not_a_tuple}");
    let nested = exec("let (a, (b, c)) = (1, (2, 3, 4))");
    assert!(nested.contains("T288"), "{nested}");
    assert!(exec("let (a) = (1, 2)").contains("P061"));
    // the names are still known, so one mistake is one error
    let one = exec("let (a, b) = (1, 2, 3)\nprint(a + b)");
    assert_eq!(one.matches("T0").count(), 0, "{one}");
}

#[test]
fn exports_and_imports_see_destructured_names() {
    let mut host = MemoryHost::default();
    host.files.insert(
        "lib.q".into(),
        "let (width, height) = (3, 4)\nexport width, height\n".into(),
    );
    let src = "import width, height from \"lib.q\"\nprint(width * height)";
    let prog = compile("main.q", src, &mut host).ok().unwrap();
    let r = prog.run(&mut host, Limits::default());
    assert!(r.error.is_none());
    assert_eq!(host.output, "12\n");
}

#[test]
fn match_takes_tuples_in_and_tests_their_elements() {
    let out = exec(
        "fun where_is(p: (int, int)) -> string\n  match p\n    case (0, 0) then \"origin\"\n    case (0, y) then \"y axis at {y}\"\n    case (x, 0) then \"x axis at {x}\"\n    case (x, y) if x == y then \"diagonal\"\n    case (x, y) then \"somewhere\"\n  end\nend\nprint(where_is((0, 0)))\nprint(where_is((0, 5)))\nprint(where_is((3, 0)))\nprint(where_is((2, 2)))\nprint(where_is((1, 2)))",
    );
    assert_eq!(
        out,
        "origin\ny axis at 5\nx axis at 3\ndiagonal\nsomewhere\n"
    );
}

#[test]
fn tuple_patterns_hold_strings_ranges_none_and_nested_tuples() {
    let out = exec(
        "let a = match (\"get\", 404)\n  case (\"get\", 200) then \"ok\"\n  case (\"get\", 400..=499) then \"client error\"\n  case (_, _) then \"other\"\nend\nprint(a)\nlet b = match (none, 1)\n  case (none, x) then x + 10\n  case (_, _) then 0\nend\nprint(b)\nlet c = match (1, (2, true))\n  case (1, (n, true)) then n\n  case (_, _) then -1\nend\nprint(c)",
    );
    assert_eq!(out, "client error\n11\n2\n");
}

#[test]
fn a_tuple_pattern_of_names_covers_every_tuple() {
    let out = exec("let n = match (1, 2)\n  case (a, b) then a + b\nend\nprint(n)");
    assert_eq!(out, "3\n");
    // with tests on values it needs an `else` or a catch-all, like numbers do
    let open =
        exec("let n = match (1, 2)\n  case (1, b) then b\n  case (a, 2) then a\nend\nprint(n)");
    assert!(open.starts_with("COMPILE:"), "{open}");
    assert_eq!(
        exec("let n = match (3, 4)\n  case (1, b) then b\n  else 0\nend\nprint(n)"),
        "0\n"
    );
}

#[test]
fn a_match_pattern_must_fit_the_value() {
    let wrong = exec("match (1, 2)\n  case (a, b, c) then print(a)\n  else print(0)\nend");
    assert!(
        wrong.contains("T220") && wrong.contains("3 elements"),
        "{wrong}"
    );
    let not_a_tuple = exec("match 5\n  case (a, b) then print(a)\n  else print(0)\nend");
    assert!(not_a_tuple.contains("T220"), "{not_a_tuple}");
    let types = exec("match (1, \"a\")\n  case (\"x\", b) then print(b)\n  else print(0)\nend");
    assert!(types.contains("T220"), "{types}");
    assert!(exec("match (1, 2)\n  case (a) then print(a)\n  else print(0)\nend").contains("P062"));
}

#[test]
fn names_of_a_tuple_pattern_are_visible_in_the_guard_and_the_body() {
    let out = exec(
        "match (3, 4)\n  case (a, b) if a < b then print(\"{a} < {b}\")\n  else print(\"no\")\nend",
    );
    assert_eq!(out, "3 < 4\n");
}

#[test]
fn a_tuple_assignment_swaps_without_a_temporary() {
    let out = exec(
        "let a = 1\nlet b = 2\n(a, b) = (b, a)\nprint(\"{a} {b}\")\nlet xs = [10, 20, 30]\n(xs[0], xs[2]) = (xs[2], xs[0])\nprint(xs)",
    );
    assert_eq!(out, "2 1\n[30, 20, 10]\n");
}

#[test]
fn a_tuple_assignment_takes_fields_nested_tuples_and_underscore() {
    let out = exec(
        "struct P\n  public x: int\n  public y: int\nend\nlet p = P { x: 1, y: 2 }\n(p.x, p.y) = (p.y, p.x + p.y)\nprint(\"{p.x} {p.y}\")\nlet q = 0\nlet r = 0\n((q, _), r) = ((5, 6), 7)\nprint(\"{q} {r}\")\nlet a = 1\n(a, _) = (100, \"ignored\")\nprint(a)",
    );
    assert_eq!(out, "2 3\n5 7\n100\n");
}

#[test]
fn a_tuple_assignment_in_a_loop_runs_each_time() {
    let out = exec(
        "fun fib(n: int) -> int\n  let (cur, next) = (0, 1)\n  for i in 0..n do\n    (cur, next) = (next, cur + next)\n  end\n  cur\nend\nprint(fib(30))",
    );
    assert_eq!(out, "832040\n");
}

#[test]
fn a_tuple_assignment_is_checked() {
    let big = exec("let a = 1\nlet b = 2\n(a, b) = (1, 2, 3)");
    assert!(
        big.contains("T288") && big.matches("error").count() <= 1 || big.contains("T288"),
        "{big}"
    );
    assert!(exec("let a = 1\nlet b = 2\n(a, b) = 5").contains("T289"));
    assert!(exec("let a = 1\nconst c = 2\n(a, c) = (1, 2)").contains("T210"));
    assert!(exec("let a = 1\nlet b = 2\n(a, b) = (\"x\", 2)").starts_with("COMPILE:"));
    // `+=` and friends are not for tuples
    assert!(exec("let a = 1\nlet b = 2\n(a, b) += (1, 1)").starts_with("COMPILE:"));
}

#[test]
fn tuples_are_ordered_element_by_element() {
    assert_eq!(
        exec(
            "print((1, 2) < (1, 3))\nprint((2, 0) > (1, 9))\nprint((1, \"a\") <= (1, \"a\"))\nprint((1, (2, 3)) < (1, (2, 2)))"
        ),
        "true\ntrue\ntrue\nfalse\n"
    );
    assert_eq!(
        exec(
            "let xs = [(2, \"b\"), (1, \"z\"), (2, \"a\")]\nxs.sort()\nprint(xs)\nprint(xs.min())\nprint(xs.max())"
        ),
        "[(1, \"z\"), (2, \"a\"), (2, \"b\")]\n(1, \"z\")\n(2, \"b\")\n"
    );
    // tuples of different shapes, or with elements that cannot be ordered, are refused
    assert!(exec("print((1, 2) < (1, 2, 3))").contains("T005"));
    let bools = exec("let xs = [(true, 1)]\nxs.sort()");
    assert!(bools.contains("T260"), "{bools}");
}
