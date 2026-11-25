//! Several `T?` locals narrowed by one condition: each use reads its own value.

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
        Err(e) => format!("{:?}", e.diagnostics),
    }
}

const ONE: &str = "fun one(n: int) -> int?
    n
end
";

#[test]
fn issue_363_and_narrows_two_locals() {
    let src = format!(
        "{ONE}fun g() -> string
    let a = one(1)
    let b = one(2)
    if a != none and b != none then
        return \"{{a}}/{{b}}\"
    end
    \"\"
end
print(g())
"
    );
    assert_eq!(run(&src), "1/2\n");
}

#[test]
fn issue_363_negated_or_and_nested_ifs() {
    let src = format!(
        "{ONE}fun g() -> string
    let a = one(1)
    let b = one(2)
    let c = one(3)
    if a == none or b == none or c == none then
        return \"none\"
    end
    \"{{a}}/{{b}}/{{c}}\"
end
fun h() -> string
    let a = one(1)
    let b = one(2)
    if not (a == none or b == none) then
        return \"{{a}}-{{b}}\"
    end
    \"\"
end
fun n() -> string
    let a = one(4)
    let b = one(5)
    if a != none then
        if b != none then
            return \"{{a}}+{{b}}\"
        end
    end
    \"\"
end
fun k() -> int
    let a = one(6)
    let b = one(7)
    if a != none and b != none then
        return b * 10 + a
    end
    0
end
print(g())
print(h())
print(n())
print(k())
"
    );
    assert_eq!(run(&src), "1/2/3\n1-2\n4+5\n76\n");
}
