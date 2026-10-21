//! `Set<T>` of the prelude.

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
fn a_set_drops_duplicates_and_keeps_the_order() {
    let out = exec(
        "let s = Set.of([3, 1, 3, 2, 1])\nprint(s)\nprint(s.len())\nprint(s.to_array())\nprint(s.is_empty())\nprint(Set<int>.new().is_empty())",
    );
    assert_eq!(out, "Set(3, 1, 2)\n3\n[3, 1, 2]\nfalse\ntrue\n");
}

#[test]
fn add_contains_remove_and_clear() {
    let out = exec(
        "let s = Set<string>.new()\nprint(s.add(\"a\"))\nprint(s.add(\"a\"))\nprint(s.contains(\"a\"))\nprint(s.contains(\"b\"))\nprint(s.remove(\"a\"))\nprint(s.remove(\"a\"))\ns.add(\"x\")\ns.clear()\nprint(s.len())",
    );
    assert_eq!(out, "true\nfalse\ntrue\nfalse\ntrue\nfalse\n0\n");
}

#[test]
fn union_intersection_difference_and_subset() {
    let out = exec(
        "let a = Set.of([1, 2, 3])\nlet b = Set.of([2, 3, 4])\nprint(a.union(b))\nprint(a.intersect(b))\nprint(a.difference(b))\nprint(b.difference(a))\nprint(Set.of([2, 3]).is_subset(a))\nprint(a.is_subset(b))\nprint(a.equals(Set.of([3, 2, 1])))\nprint(a.equals(b))\nprint(a)",
    );
    assert_eq!(
        out,
        "Set(1, 2, 3, 4)\nSet(2, 3)\nSet(1)\nSet(4)\ntrue\nfalse\ntrue\nfalse\nSet(1, 2, 3)\n"
    );
}

#[test]
fn a_set_is_looped_over_and_converted() {
    let out = exec(
        "let s = Set.of([\"b\", \"a\", \"b\"])\nlet total = 0\nfor w in s do\n  total += w.len()\n  write(\"{w} \")\nend\nprint(\"\")\nprint(total)\nprint(\"{s}\")\nprint(s as string)",
    );
    assert_eq!(out, "b a \n2\nSet(b, a)\nSet(b, a)\n");
}

#[test]
fn values_follow_the_rules_of_map_keys() {
    let out = exec(
        "enum Color\n  Red\n  Green\nend\nlet s = Set.of([Color.Red, Color.Green, Color.Red])\nprint(s.len())\nprint(Set.of([1.5, 2.5]).len())\nprint(Set.of([true, false, true]).len())",
    );
    assert_eq!(out, "2\n2\n2\n");
    let bad = exec("struct P\n  public x: int\nend\nlet s: Set<P> = Set<P>.new()");
    assert!(bad.contains("T270"), "{bad}");
    let bad = exec("let s: Set<array<int>> = Set<array<int>>.new()");
    assert!(bad.contains("T270"), "{bad}");
}

#[test]
fn a_set_changed_during_the_loop_is_not_a_problem() {
    let out = exec("let s = Set.of([1, 2, 3])\nfor v in s do\n  s.add(v * 10)\nend\nprint(s)");
    assert_eq!(out, "Set(1, 2, 3, 10, 20, 30)\n");
}

#[test]
fn types_are_checked() {
    assert!(exec("let s = Set.of([1, 2])\ns.add(\"a\")").starts_with("COMPILE:"));
    assert!(exec("let s = Set.of([1, 2])\nfor a, b in s do end").starts_with("COMPILE:"));
    let out = exec(
        "fun uniq(xs: array<int>) -> Set<int>\n  Set.of(xs)\nend\nprint(uniq([1, 1, 2]).len())",
    );
    assert_eq!(out, "2\n");
}
