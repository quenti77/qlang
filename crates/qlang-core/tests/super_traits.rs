//! `trait B extends A`: whoever implements `B` implements `A`, and a `B` can be used as an `A`.

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

const BASE: &str = "trait Named
  fun name(self) -> string
end
trait Greeter extends Named
  fun greet(self) -> string
    \"hello {self.name()}\"
  end
end
struct Dog
  public label: string
end
impl Named for Dog
  fun name(self) -> string
    self.label
  end
end
impl Greeter for Dog
end
";

#[test]
fn the_methods_of_the_extended_trait_are_available() {
    let src = format!(
        "{BASE}
fun both(g: Greeter) -> string
  \"{{g.greet()}} / {{g.name()}}\"
end
fun generic<T: Greeter>(x: T) -> string
  x.name()
end
fun as_named(n: Named) -> string
  n.name()
end
let d = Dog {{ label: \"Rex\" }}
print(both(d))
print(generic(d))
let g: Greeter = d
print(as_named(g))
"
    );
    assert_eq!(run(&src), "hello Rex / Rex\nRex\nRex\n");
}

#[test]
fn an_impl_must_also_implement_the_extended_trait() {
    let src = "trait Named
  fun name(self) -> string
end
trait Greeter extends Named
  fun greet(self) -> string
end
struct Cat
  public label: string
end
impl Greeter for Cat
  fun greet(self) -> string
    \"mew\"
  end
end
";
    assert_eq!(run(src), "T136");
}

#[test]
fn only_a_trait_can_be_extended_and_never_itself() {
    assert_eq!(run("trait A extends A\nend\n"), "T136");
    assert_eq!(
        run("struct S\n  public x: int\nend\ntrait A extends S\nend\n"),
        "T136"
    );
}

#[test]
fn several_traits_and_a_chain() {
    let src = "trait A
  fun a(self) -> int
end
trait B
  fun b(self) -> int
end
trait C extends A, B
  fun c(self) -> int
end
trait D extends C
end
struct S
  public n: int
end
impl A for S
  fun a(self) -> int
    1
  end
end
impl B for S
  fun b(self) -> int
    2
  end
end
impl C for S
  fun c(self) -> int
    3
  end
end
impl D for S
end
fun total(d: D) -> int
  d.a() + d.b() + d.c()
end
print(total(S { n: 0 }))
";
    assert_eq!(run(src), "6\n");
}
