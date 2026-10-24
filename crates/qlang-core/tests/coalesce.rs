//! `a ?? b`: the value of `a`, or `b` when `a` is `none`.

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

const PICK: &str = "fun pick(i: int) -> int?
  if i mod 3 == 0 then
    return none
  end
  i
end
";

#[test]
fn falls_back_only_on_none() {
    let src = format!(
        "{PICK}print(\"{{pick(3) ?? 7}} {{pick(4) ?? 7}}\")
let zero: int? = 0
print(zero ?? 5)
"
    );
    assert_eq!(run(&src), "7 4\n0\n");
}

#[test]
fn chains_to_the_right_and_may_stay_nullable() {
    let src = format!(
        "{PICK}let none_int: int? = none
let a = none_int ?? pick(3) ?? 9
let b: int? = none_int ?? pick(3)
print(\"{{a}} {{b}}\")
"
    );
    assert_eq!(run(&src), "9 none\n");
}

#[test]
fn right_side_is_lazy() {
    let src = "fun boom() -> int
  let xs: array<int> = []
  xs[3]
end
let v: int? = 1
print(v ?? boom())
";
    assert_eq!(run(src), "1\n");
}

#[test]
fn works_in_a_hot_loop() {
    let src = format!(
        "{PICK}let t = 0
for i in 0..30 do
  t += pick(i) ?? 1
end
print(t)
"
    );
    // 10 multiples of three count 1, the others count i
    assert_eq!(run(&src), "310\n");
}

#[test]
fn needs_a_nullable_left_side_and_a_matching_right_side() {
    assert_eq!(run("let a = 1\nprint(a ?? 2)\n"), "T010");
    assert_eq!(run("let a: int? = 1\nlet b: int = a ?? \"x\"\n"), "T002");
    assert_eq!(run("let a: int?? = 1\n"), "P030");
}
