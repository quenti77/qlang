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

#[test]
fn a_bound_on_a_trait_gives_the_operators_of_its_super_traits() {
    let src = "trait Numeric<T> extends Add<T>, Sub<T>
end
impl Numeric<int> for int
end
fun double<T: Numeric<T>>(a: T) -> T
    a + a
end
fun gap<T: Numeric<T> + Ord>(a: T, b: T) -> T
    if a > b then a - b else b - a end
end
print(double(2))
print(gap(2, 9))
";
    assert_eq!(run(src), "4\n7\n");
}

#[test]
fn sum_adds_elements_that_implement_add() {
    let src = "struct V
    public x: int
end
impl Add<V> for V
    public fun add(self, o: V) -> V
        V { x: self.x + o.x }
    end
end
fun total<T: Add<T>>(xs: array<T>) -> T
    xs.sum()
end
print([V { x: 1 }, V { x: 2 }, V { x: 3 }].sum().x)
print(total([V { x: 4 }, V { x: 5 }]).x)
print(total([1, 2, 3]))
print([1, 2].sum())
print([1.5, 2.0].sum())
";
    assert_eq!(run(src), "6\n9\n6\n3\n3.5\n");
}

#[test]
fn sum_of_elements_that_cannot_be_added_is_refused_and_an_empty_one_has_no_zero() {
    assert_eq!(
        run("struct V\n  public x: int\nend\nprint([V { x: 1 }].sum())\n"),
        "T260"
    );
    let src = "struct V
    public x: int
end
impl Add<V> for V
    public fun add(self, o: V) -> V
        V { x: self.x + o.x }
    end
end
let empty: array<V> = []
print(empty.sum())
";
    assert_eq!(run(src), "RUNTIME R015");
}

#[test]
fn the_methods_of_the_extended_trait_can_be_given_in_the_impl_of_the_trait_that_extends_it() {
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
    \"mew \" + self.name()
  end
  fun name(self) -> string
    self.label
  end
end
fun hello(g: Greeter) -> string
  g.greet() + \" / \" + g.name()
end
fun named(n: Named) -> string
  n.name()
end
let c = Cat { label: \"tom\" }
print(hello(c))
print(named(c))
";
    assert_eq!(run(src), "mew tom / tom\ntom\n");
}

#[test]
fn a_chain_of_traits_can_be_implemented_in_one_impl() {
    let src = "trait A
  fun a(self) -> int
end
trait B extends A
  fun b(self) -> int
end
trait C extends B
  fun c(self) -> int
end
struct S
end
impl C for S
  fun a(self) -> int
    1
  end
  fun b(self) -> int
    20
  end
  fun c(self) -> int
    300
  end
end
fun total<T: C>(x: T) -> int
  x.a() + x.b() + x.c()
end
print(total(S {}))
";
    assert_eq!(run(src), "321\n");
}

#[test]
fn the_order_of_the_impls_does_not_matter() {
    let src = "trait Named
  fun name(self) -> string
end
trait Greeter extends Named
end
struct Cat
end
impl Greeter for Cat
end
impl Named for Cat
  fun name(self) -> string
    \"cat\"
  end
end
fun hello(g: Greeter) -> string
  g.name()
end
print(hello(Cat {}))
";
    assert_eq!(run(src), "cat\n");
}

#[test]
fn a_method_of_the_extended_trait_is_missing_or_given_twice() {
    let base = "trait Named
  fun name(self) -> string
  fun other(self) -> string
end
trait Greeter extends Named
end
struct Cat
end
";
    // one of the two methods of `Named` is missing
    let src =
        format!("{base}impl Greeter for Cat\n  fun name(self) -> string\n    \"x\"\n  end\nend\n");
    assert_eq!(run(&src), "T133");
    // methods of `Named` here, and an `impl Named` of its own
    let src = format!(
        "{base}impl Named for Cat\n  fun name(self) -> string\n    \"x\"\n  end\n  fun other(self) -> string\n    \"y\"\n  end\nend\nimpl Greeter for Cat\n  fun name(self) -> string\n    \"z\"\n  end\nend\n"
    );
    assert_eq!(run(&src), "T137");
    // a method of no trait at all
    let src = format!(
        "{base}impl Greeter for Cat\n  fun name(self) -> string\n    \"x\"\n  end\n  fun other(self) -> string\n    \"y\"\n  end\n  fun nothing(self) -> string\n    \"z\"\n  end\nend\n"
    );
    assert_eq!(run(&src), "T135");
}
