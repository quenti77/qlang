//! The native code (feature `jit`) must give what the tree walker gives, errors included: in
//! each program a function becomes hot (300 calls) and then meets a limit or an error, and the
//! output (message, line, call stack) must be the same.
#![cfg(feature = "jit")]

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str, limits: Limits, walker: bool) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => return format!("COMPILE: {}", e.render()),
            };
            let r = if walker {
                prog.run_tree_walker(&mut host, limits)
            } else {
                prog.run(&mut host, limits)
            };
            match r.error {
                Some(d) => format!("{}{}", host.output, d.render(&prog.sources)),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn same_with(src: &str, limits: Limits) -> String {
    let a = run(src, limits, false);
    let b = run(src, limits, true);
    assert_eq!(a, b, "the engines disagree on:\n{src}");
    a
}

fn same(src: &str) -> String {
    same_with(src, Limits::default())
}

#[test]
fn a_hot_recursive_function_gives_the_same_result() {
    let out = same(
        "fun fib(n: int) -> int\n  if n < 2 then n else fib(n - 1) + fib(n - 2) end\nend\nprint(fib(25))",
    );
    assert_eq!(out, "75025\n");
}

#[test]
fn an_overflow_in_native_code_is_reported_as_the_machine_does() {
    let out = same(
        "fun sq(n: int) -> int\n  n * n\nend\nlet t = 0\nfor i in 0..100000 do\n  t += sq(i * 100000)\nend\nprint(t)",
    );
    assert!(out.contains("R010"), "{out}");
}

#[test]
fn a_division_by_zero_in_native_code_is_reported_as_the_machine_does() {
    let out = same(
        "fun d(a: int, b: int) -> int\n  a // b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += d(100, 500 - i)\nend\nprint(t)",
    );
    assert!(out.contains("R011"), "{out}");
}

#[test]
fn negative_division_and_modulo_agree() {
    let out = same(
        "fun g(a: int, b: int) -> int\n  a // b + a mod b\nend\nlet t = 0\nfor i in -400..400 do\n  t += g(i * 7, 5) + g(i * 7, -3)\nend\nprint(t)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_infinite_recursion_stops_at_the_same_depth() {
    let out = same(
        "fun r(n: int) -> int\n  if n < 100 then n else r(n + 1) + 1 end\nend\nlet t = 0\nfor i in 0..500 do\n  t += r(i)\nend\nprint(t)\nfun inf(n: int) -> int\n  inf(n + 1) + 1\nend\nprint(inf(0))",
    );
    assert!(out.contains("R901"), "{out}");
}

#[test]
fn the_step_limit_stops_a_hot_function_where_the_machine_does() {
    // the tree walker reports the line of the statement in the loop, the machine that of the
    // loop: native code must say what the machine says
    let limits = Limits {
        max_steps: 200_000,
        ..Limits::default()
    };
    let out = run(
        "fun s(n: int) -> int\n  let i = 0\n  while i < n do\n    i += 1\n  end\n  i\nend\nlet t = 0\nfor k in 0..100000 do\n  t += s(100)\nend\nprint(t)",
        limits,
        false,
    );
    assert!(out.contains("R900") && out.contains("main.q:3:3"), "{out}");
}

#[test]
fn floats_comparisons_and_for_loops_agree() {
    let out = same(
        "fun h(x: float) -> float\n  if x < 0.5 then x * 2.0 else x - 1.5 end\nend\nfun f(n: int) -> float\n  let s = 0.0\n  for i in 0..n do\n    s += h(i / 7)\n  end\n  s\nend\nfor k in 0..400 do\n  f(10)\nend\nprint(f(1000))\nfun w(a: int, b: int) -> int\n  let c = 0\n  for i in a..b step 3 do\n    c += i\n  end\n  c\nend\nfor k in 0..400 do\n  w(0, 10)\nend\nprint(w(-50, 50))\nprint(w(50, -50))",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_float_division_by_zero_in_native_code_is_reported() {
    let out = same(
        "fun q(x: float) -> float\n  1.0 / (x - 1.0)\nend\nlet t = 0.0\nfor i in 0..1000 do\n  t += q(i * 1.0)\nend\nprint(t)",
    );
    assert!(out.contains("R011"), "{out}");
}

#[test]
fn powers_roots_min_max_and_float_modulo_agree() {
    let out = same(
        "fun a(x: int, y: int) -> int\n  (x ** y).min(1000000) + x.max(y)\nend\nfun b(x: float, y: float) -> float\n  x.sqrt() + (x ** y) + x.min(y) + x.max(y) + x mod y\nend\nlet t = 0\nlet u = 0.0\nfor i in 0..1000 do\n  t += a(i mod 7, i mod 5)\n  u += b(i * 1.0, 1.5 + i mod 3)\nend\nprint(t)\nprint(u)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_power_overflow_a_negative_exponent_and_a_root_of_a_negative_are_reported() {
    let o = same(
        "fun p(a: int, b: int) -> int\n  a ** b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += p(3, i mod 41)\nend\nprint(t)",
    );
    assert!(o.contains("R010"), "{o}");
    let o = same(
        "fun p(a: int, b: int) -> int\n  a ** b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += p(2, 5 - i)\nend\nprint(t)",
    );
    assert!(o.contains("R012"), "{o}");
    let o = same(
        "fun r(x: float) -> float\n  x.sqrt()\nend\nlet t = 0.0\nfor i in 0..1000 do\n  t += r(500.0 - i)\nend\nprint(t)",
    );
    assert!(o.contains("R014"), "{o}");
}
