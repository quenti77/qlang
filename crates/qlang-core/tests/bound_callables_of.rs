//! `Reflect.bound_callables_of(value)`: the methods of the struct an `Any` holds, without naming it.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {} {}", d.code, d.message),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => e.render(),
    }
}

#[test]
fn the_methods_of_an_any_are_bound_to_it() {
    let src = "struct Ctl
  public n: int
end
impl Ctl
  public fun hello(self) -> string
    \"hi {self.n}\"
  end
  fun hidden(self) -> int
    1
  end
end
fun hold() -> Any
  Any.of(Ctl { n: 3 })
end
let found = Reflect.bound_callables_of(hold())
print(found.len())
print(found[0].name)
print(found[0].call([]))
print(Reflect.bound_callables_of(Any.of(5)).len())
";
    assert_eq!(run(src), "1\nhello\nResult.Ok(hi 3)\n0\n");
}
