//! Values nested very deeply (a linked list of a million nodes) must not overflow the stack of
//! the thread that drops or compares them: that would take a whole server down.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// Runs on a small stack (1 MB, like a browser's): a recursion per level would not fit.
fn exec_small_stack(src: &str, max_steps: u64) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => return format!("COMPILE: {}", e.render()),
            };
            let limits = Limits {
                max_steps,
                max_depth: 100,
                ..Limits::default()
            };
            let r = prog.run(&mut host, limits);
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
fn a_long_linked_list_of_structs_is_dropped_without_recursing() {
    let out = exec_small_stack(
        "struct N\n  public next: N?\nend\nlet head: N? = none\nfor i in 0..200000 do\n  head = N { next: head }\nend\nprint(\"built\")\nhead = none\nprint(\"dropped\")",
        100_000_000,
    );
    assert_eq!(out, "built\ndropped\n");
}

#[test]
fn nested_arrays_and_enums_are_dropped_without_recursing() {
    let out = exec_small_stack(
        "struct B\n  public items: array<B>\nend\nlet x = B { items: [] }\nfor i in 0..100000 do\n  x = B { items: [x] }\nend\nx = B { items: [] }\nprint(\"arrays\")",
        100_000_000,
    );
    assert_eq!(out, "arrays\n");
    let out = exec_small_stack(
        "enum L\n  Nil\n  Cons(next: L)\nend\nlet a = L.Nil\nfor i in 0..200000 do\n  a = L.Cons(a)\nend\na = L.Nil\nprint(\"enums\")",
        100_000_000,
    );
    assert_eq!(out, "enums\n");
}

#[test]
fn a_chain_of_closures_is_dropped_without_recursing() {
    let out = exec_small_stack(
        "fun wrap(f: fun() -> int) -> fun() -> int\n  fun() -> int\n    f() + 1\n  end\nend\nlet f = fun() -> int\n  0\nend\nfor i in 0..100000 do\n  f = wrap(f)\nend\nf = fun() -> int\n  0\nend\nprint(\"closures\")",
        100_000_000,
    );
    assert_eq!(out, "closures\n");
}

#[test]
fn comparing_values_nested_too_deeply_is_an_error() {
    let out = exec_small_stack(
        "enum L\n  Nil\n  Cons(next: L)\nend\nlet a = L.Nil\nlet b = L.Nil\nfor i in 0..100000 do\n  a = L.Cons(a)\n  b = L.Cons(b)\nend\nprint(a == b)",
        100_000_000,
    );
    assert!(out.starts_with("R092"), "{out}");
    // a normal comparison still works
    let out = exec_small_stack(
        "enum L\n  Nil\n  Cons(next: L)\nend\nlet a = L.Cons(L.Cons(L.Nil))\nlet b = L.Cons(L.Cons(L.Nil))\nprint(a == b)\nprint((1, (2, 3)) < (1, (2, 4)))",
        1_000_000,
    );
    assert_eq!(out, "true\ntrue\n");
}
