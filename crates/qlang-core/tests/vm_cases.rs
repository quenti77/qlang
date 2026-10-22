//! Hand-written cases for the typed register machine: each program must give the same result
//! on the machine and on the tree walker, and the result is also checked against what it
//! should be. (`vm_differential.rs` does the same with random programs.)

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn both(src: &str, limits: Limits) -> (String, String, (usize, usize)) {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let go = |vm: bool| {
                let mut host = MemoryHost::default();
                let prog = match compile("main.q", &src, &mut host) {
                    Ok(p) => p,
                    Err(e) => return (format!("COMPILE: {}", e.render()), (0, 0)),
                };
                let r = if vm {
                    prog.run(&mut host, limits)
                } else {
                    prog.run_tree_walker(&mut host, limits)
                };
                let out = match r.error {
                    Some(d) => format!("{}{}", host.output, d.render(&prog.sources)),
                    None => host.output,
                };
                (out, r.vm_functions)
            };
            let (a, fa) = go(true);
            let (b, _) = go(false);
            (a, b, fa)
        })
        .unwrap()
        .join()
        .unwrap()
}

/// The output, after checking that both engines agree and that the machine was used.
fn same(src: &str) -> String {
    let (vm, walker, used) = both(src, Limits::default());
    assert_eq!(vm, walker, "the engines disagree on:\n{src}");
    assert!(
        used.0 > 0,
        "the machine compiled nothing in this program:\n{src}"
    );
    vm
}

#[test]
fn recursion_loops_and_mixed_numbers() {
    assert_eq!(
        same(
            "fun fib(n: int) -> int\n  if n < 2 then n else fib(n - 1) + fib(n - 2) end\nend\nprint(fib(20))"
        ),
        "6765\n"
    );
    assert_eq!(
        same(
            "fun f(n: int) -> float\n  let s = 0.0\n  for i in 1..=n do\n    s += i / 2 + 1.5\n  end\n  s\nend\nprint(f(4))"
        ),
        "11.0\n"
    );
    assert_eq!(
        same(
            "fun f(a: int, b: float) -> bool\n  a * b > 7 and (a as float) == 3.0\nend\nprint(f(3, 2.5))\nprint(f(2, 2.5))"
        ),
        "true\nfalse\n"
    );
    assert_eq!(
        same(
            "fun f(x: float) -> int\n  (x * 2.5) as int + x.floor() + x.sqrt().round()\nend\nprint(f(9.0))"
        ),
        "34\n"
    );
}

#[test]
fn errors_are_the_same_with_the_same_trace() {
    let out = same(
        "fun f(a: int) -> int\n  9223372036854775807 + a\nend\nfun g(a: int) -> int\n  f(a) * 2\nend\nprint(g(1))",
    );
    assert!(
        out.contains("R010") && out.contains("called from `f`"),
        "{out}"
    );
    let out = same("fun f(a: int) -> int\n  10 div a\nend\nprint(f(0))");
    assert!(out.contains("R011"), "{out}");
    let out = same("fun f(x: float) -> float\n  x.sqrt()\nend\nprint(f(-1.0))");
    assert!(out.contains("R014"), "{out}");
    let out = same("fun f(x: float) -> int\n  x as int\nend\nprint(f(1e300))");
    assert!(out.contains("R013"), "{out}");
    let out = same("fun f(a: int) -> int\n  2 ** a\nend\nprint(f(-1))");
    assert!(out.contains("R012"), "{out}");
    let out = same(
        "fun f(n: int) -> int\n  let t = 0\n  for i in 0..10 step n do\n    t += i\n  end\n  t\nend\nprint(f(0))",
    );
    assert!(out.contains("R031"), "{out}");
}

#[test]
fn limits_apply_inside_the_machine() {
    let (vm, walker, _) = both(
        "fun spin() -> int\n  let i = 0\n  while true do\n    i += 1\n  end\n  i\nend\nprint(spin())",
        Limits {
            max_steps: 100_000,
            ..Limits::default()
        },
    );
    assert!(vm.contains("R900") && walker.contains("R900"), "{vm}");
    let (vm, walker, _) = both(
        "fun deep(n: int) -> int\n  deep(n + 1) + 1\nend\nprint(deep(0))",
        Limits::default(),
    );
    assert!(vm.contains("R901") && walker.contains("R901"), "{vm}");
}

#[test]
fn a_loop_shares_variables_with_the_code_around_it() {
    // module variables, assigned by the loop and read afterwards
    assert_eq!(
        same(
            "let total = 0\nlet i = 0\nwhile i < 1000 do\n  total += i mod 7\n  i += 1\nend\nprint(total)\nprint(i)"
        ),
        "2997\n1000\n"
    );
    // locals of a function the machine does not compile (it prints)
    assert_eq!(
        same(
            "fun f(n: int) -> int\n  let a = 0\n  let b = 1\n  for _k in 0..n do\n    (a, b) = (b, a + b)\n  end\n  print(\"done\")\n  a\nend\nprint(f(10))"
        ),
        "done\n55\n"
    );
    // break and continue, a nested loop, a variable of the loop shadowing an outer one
    assert_eq!(
        same(
            "let x = 100\nlet s = 0\nfor i in 0..50 do\n  if i mod 2 == 0 then continue end\n  let x = i\n  for j in 0..i do\n    if j > 3 then break end\n    s += x * j\n  end\nend\nprint(s)\nprint(x)"
        ),
        "3735\n100\n"
    );
    // an inclusive range up to the biggest int does not overflow
    assert_eq!(
        same(
            "let n = 0\nfor i in 9223372036854775806..=9223372036854775807 do\n  n += 1\nend\nprint(n)"
        ),
        "2\n"
    );
    // a call inside the loop that changes a module variable the loop uses
    assert_eq!(
        same(
            "let g = 0\nfun bump(x: int) -> int\n  g += x\n  g\nend\nlet t = 0\nfor i in 1..=5 do\n  t += bump(i)\n  g += 1\nend\nprint(t)\nprint(g)"
        ),
        "45\n20\n"
    );
}

#[test]
fn what_the_machine_cannot_do_still_runs() {
    // text, arrays, structures: the tree walker, with the numeric parts around it
    assert_eq!(
        same(
            "fun sq(x: int) -> int\n  x * x\nend\nlet xs = [1, 2, 3]\nlet total = 0\nfor x in xs do\n  total += sq(x)\nend\nprint(total)\nprint(\"{sq(4)} items\")"
        ),
        "14\n16 items\n"
    );
}
