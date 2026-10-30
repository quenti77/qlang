//! `-9223372036854775808`, the smallest integer, can be written as a literal.

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
fn the_smallest_integer_is_a_literal() {
    let src = "let low = -9223372036854775808
print(low)
print(low == -9223372036854775807 - 1)
print(-9223372036854775807)
match low
  case -9223372036854775808 then print(\"min\")
  else print(\"other\")
end
";
    assert_eq!(
        run(src),
        "-9223372036854775808\ntrue\n-9223372036854775807\nmin\n"
    );
}

#[test]
fn the_unsigned_digits_alone_stay_too_large() {
    assert_eq!(run("let x = 9223372036854775808\n"), "L005");
    assert_eq!(run("let a = 1\nlet x = a - 9223372036854775808\n"), "L005");
    assert_eq!(run("let x = 9223372036854775809\n"), "L005");
    assert_eq!(run("let x = -9223372036854775809\n"), "L005");
}
