//! `Panic.guard`: a failure inside a function is returned instead of stopping the program.

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
            let r = prog.run(
                &mut host,
                Limits {
                    max_steps: 200_000,
                    ..Limits::default()
                },
            );
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
fn a_failed_assertion_is_returned_with_where_it_happened() {
    let out = exec(
        r#"
fun boom(x: int) -> int
  assert_eq(x, 3)
  x
end
let failure = Panic.guard(fun() boom(1) end)
if failure != none then
  print(failure.code)
  print(failure.message.contains("assert_eq failed"))
  print(failure.places.len() >= 2)
  print(failure.places[0])
end
print(Panic.guard(fun() boom(3) end) == none)
"#,
    );
    assert_eq!(out, "R050\ntrue\ntrue\nmain.q:3\ntrue\n");
}

#[test]
fn the_program_goes_on_and_failures_do_not_pile_up() {
    let out = exec(
        r#"
let count = 0
let result: Result<int, Error> = Result<int, Error>.Err(Error { message: "bad" })
for i in 0..50 do
  let failure = Panic.guard(fun()
    count += 1
    if i % 2 == 0 then
      result.unwrap()
    end
  end)
  if i % 2 == 0 and failure == none then
    print("missed {i}")
  end
end
print(count)
let nested = Panic.guard(fun()
  let inner = Panic.guard(fun() panic("inner") end)
  print(inner != none)
end)
print(nested == none)
let array_failure = Panic.guard(fun()
  let a: array<int> = []
  print(a[3])
end)
print(array_failure != none)
"#,
    );
    assert_eq!(out, "50\ntrue\ntrue\ntrue\n");
}

#[test]
fn running_out_of_steps_is_not_a_failure_to_catch() {
    let out = exec(
        r#"
let failure = Panic.guard(fun()
  while true do
  end
end)
print("not reached")
"#,
    );
    assert!(out.starts_with("R90"), "{out}");
}
