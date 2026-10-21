//! Variable lookups remember where they found their variable; these programs make the same
//! place in the code see different scopes, to check that the memory never gives a wrong answer.

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
fn a_name_used_before_a_shadowing_declaration_in_a_loop() {
    let out = exec(
        "let y = 100\nlet i = 0\nwhile i < 3 do\n  print(y)\n  let y = i\n  print(y)\n  i += 1\nend",
    );
    assert_eq!(out, "100\n0\n100\n1\n100\n2\n");
}

#[test]
fn the_same_function_called_from_different_scopes() {
    let out = exec(
        "let x = 1\nfun f() -> int\n  x\nend\nfun g() -> int\n  let x = 2\n  f() + x\nend\nprint(f())\nprint(g())\nlet x2 = 5\nprint(f() + x2)",
    );
    assert_eq!(out, "1\n3\n6\n");
}

#[test]
fn closures_keep_their_own_variables() {
    let out = exec(
        "fun counter() -> fun() -> int\n  let n = 0\n  fun() -> int\n    n += 1\n    n\n  end\nend\nlet a = counter()\nlet b = counter()\nprint(a())\nprint(a())\nprint(b())\nprint(a())",
    );
    assert_eq!(out, "1\n2\n1\n3\n");
}

#[test]
fn recursion_and_compound_assignment() {
    let out = exec(
        "let total = 0\nfun add(n: int)\n  if n == 0 then return end\n  total += n\n  add(n - 1)\nend\nadd(10)\nprint(total)\nlet i = 0\nfor k in 0..5 do\n  let i = k * 2\n  total += i\nend\nprint(total)\nprint(i)",
    );
    assert_eq!(out, "55\n75\n0\n");
}

#[test]
fn match_cases_bind_in_their_own_scope() {
    let out = exec(
        "fun f(v: int?) -> int\n  match v\n    case none then 0\n    case n if n > 5 then n * 2\n    case n then n\n  end\nend\nlet n = 1000\nprint(f(none))\nprint(f(7))\nprint(f(3))\nprint(n)",
    );
    assert_eq!(out, "0\n14\n3\n1000\n");
}

#[test]
fn integer_fast_paths_keep_the_errors() {
    assert!(exec("print(9223372036854775807 + 1)").contains("R010"));
    assert!(exec("let a = 1\nprint(a mod 0)").contains("R011"));
    assert!(exec("let a = 1\nprint(a div 0)").contains("R011"));
    assert_eq!(
        exec("print(7 div 2)\nprint(-7 mod 3)\nprint(2 < 3)\nprint(2 == 2)"),
        "3\n-1\ntrue\ntrue\n"
    );
}
