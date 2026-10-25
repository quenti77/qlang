//! `fun f(a: int, b: int = 3)`: a call may leave out a parameter that has a default.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
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
fn a_call_leaves_out_what_has_a_default() {
    let src = "fun f(a: string, b: int = 3, c: string? = none, d: array<int> = [], e: float = -1.5) -> string
  \"{a} {b} {c ?? \"-\"} {d.len()} {e}\"
end
print(f(\"x\"))
print(f(\"x\", 5))
print(f(\"x\", c: \"z\"))
print(f(\"x\", d: [1, 2], b: 9))
";
    assert_eq!(
        run(src),
        "x 3 - 0 -1.5\nx 5 - 0 -1.5\nx 3 z 0 -1.5\nx 9 - 2 -1.5\n"
    );
}

#[test]
fn methods_take_defaults_too() {
    let src = "struct S
end
impl S
  public fun m(self, x: int, y: int = 10) -> int
    x + y
  end
end
let s = S {}
print(s.m(1))
print(s.m(1, 2))
";
    assert_eq!(run(src), "11\n3\n");
}

#[test]
fn a_generic_parameter_only_a_default_mentions_needs_no_type() {
    let src = "fun note<C>(m: string, context: C? = none)
  if context == none then
    print(m)
  else
    print(\"{m} {Json.of(context).to_text()}\")
  end
end
note(\"a\")
note(\"b\", {\"x\": 1})
";
    assert_eq!(run(src), "a\nb {\"x\":1}\n");
}

#[test]
fn what_cannot_be_left_out_is_reported() {
    // `a` has no default
    assert_eq!(
        run("fun f(a: int, b: int = 1) -> int\n  a + b\nend\nprint(f())\n"),
        "T240"
    );
    // too many
    assert_eq!(
        run("fun f(a: int, b: int = 1) -> int\n  a + b\nend\nprint(f(1, 2, 3))\n"),
        "T240"
    );
    // the default has the wrong type
    assert_eq!(
        run("fun f(a: int = \"x\") -> int\n  a\nend\nprint(f())\n"),
        "T002"
    );
    // only a literal
    assert!(
        run("fun g() -> int\n  1\nend\nfun f(a: int = g()) -> int\n  a\nend\nprint(f())\n")
            .contains("P043")
    );
}
