//! `T.zero()`: a method without `self` called through a type parameter; `Self` in traits and impls;
//! the prelude traits `Zero` and `One`; `sum()` of an empty array of `Zero` elements.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// The output of the program, or `COMPILE codes` / `RUNTIME code`.
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
            format!("COMPILE {}", codes.join(","))
        }
    }
}

const ZERO: &str = "trait Zero2
    fun zero() -> Self
end
impl Zero2 for int
    fun zero() -> Self
        0
    end
end
fun zero_of<T: Zero2>() -> T
    T.zero()
end
";

#[test]
fn a_static_method_is_called_through_a_type_parameter() {
    let src = format!("{ZERO}print(zero_of<int>())\n");
    assert_eq!(run(&src), "0\n");
}

#[test]
fn the_type_argument_goes_through_the_functions_that_pass_it_on() {
    let src = format!(
        "{ZERO}fun twice<T: Zero2>() -> array<T>\n    [zero_of<T>(), zero_of<T>()]\nend\nprint(twice<int>())\n"
    );
    assert_eq!(run(&src), "[0, 0]\n");
}

#[test]
fn an_impl_method_gets_the_type_arguments_of_its_impl() {
    let src = "struct Point<T>
    public x: T
    public y: T
end
impl<T: Zero> Point<T>
    public fun origin() -> Point<T>
        Point { x: T.zero(), y: T.zero() }
    end
end
print(Point<int>.origin())
let p: Point<float> = Point.origin()
print(p)
";
    assert_eq!(run(src), "Point { x: 0, y: 0 }\nPoint { x: 0.0, y: 0.0 }\n");
}

#[test]
fn an_impl_for_a_generic_type_calls_the_method_of_its_parameter() {
    let src = "struct Pair<T>
    public a: T
    public b: T
end
impl<T: Zero> Zero for Pair<T>
    public fun zero() -> Self
        Pair { a: T.zero(), b: T.zero() }
    end
end
fun zero_of<T: Zero>() -> T
    T.zero()
end
print(zero_of<Pair<Pair<int>>>())
";
    assert_eq!(
        run(src),
        "Pair { a: Pair { a: 0, b: 0 }, b: Pair { a: 0, b: 0 } }\n"
    );
}

#[test]
fn self_is_the_type_that_implements_the_trait() {
    let src = "struct Money
    public cents: int
end
impl Zero for Money
    public fun zero() -> Self
        Money { cents: 0 }
    end
end
fun zero_of<T: Zero>() -> T
    T.zero()
end
print(zero_of<Money>())
";
    assert_eq!(run(src), "Money { cents: 0 }\n");
}

#[test]
fn a_trait_can_extend_operators_on_self() {
    let src = "trait Numeric extends Add<Self>, Zero
    fun double(self) -> Self
end
impl Numeric for int
    public fun double(self) -> Self
        self + self
    end
end
fun sum_all<T: Numeric>(xs: array<T>) -> T
    let t = T.zero()
    for x in xs do
        t = t + x.double()
    end
    t
end
print(sum_all([1, 2, 3]))
";
    assert_eq!(run(src), "12\n");
}

#[test]
fn zero_and_one_are_in_the_prelude_for_int_and_float() {
    let src = "fun total<T: Add + Zero>(xs: array<T>) -> T
    let t = T.zero()
    for x in xs do
        t = t + x
    end
    t
end
fun product<T: Mul + One>(xs: array<T>) -> T
    let t = T.one()
    for x in xs do
        t = t * x
    end
    t
end
print(total([1, 2, 3]))
print(total([1.5, 2.5]))
print(total<int>([]))
print(product([2, 3, 4]))
print(product<float>([]))
";
    assert_eq!(run(src), "6\n4.0\n0\n24\n1.0\n");
}

#[test]
fn the_sum_of_an_empty_array_is_zero_when_the_elements_have_one() {
    let money = "struct Money
    public cents: int
end
impl Add for Money
    public fun add(self, other: Money) -> Money
        Money { cents: self.cents + other.cents }
    end
end
";
    let zero = "impl Zero for Money
    public fun zero() -> Self
        Money { cents: 0 }
    end
end
";
    let src = format!(
        "{money}{zero}fun total<T: Add + Zero>(xs: array<T>) -> T\n    xs.sum()\nend\nlet nothing: array<Money> = []\nprint(nothing.sum())\nprint(total<Money>([]))\nprint(total<int>([]))\n"
    );
    assert_eq!(run(&src), "Money { cents: 0 }\nMoney { cents: 0 }\n0\n");
    // without `Zero` an empty sum still has no value
    let src = format!("{money}let nothing: array<Money> = []\nprint(nothing.sum())\n");
    assert_eq!(run(&src), "RUNTIME R015");
}

#[test]
fn errors() {
    // the bound does not declare the method
    assert_eq!(
        run("fun bad<T>() -> T\n    T.zero()\nend\n"),
        "COMPILE T161"
    );
    assert_eq!(
        run("fun bad<T: Zero>() -> T\n    T.nothing()\nend\n"),
        "COMPILE T161"
    );
    // `T` cannot be known: it is not in the arguments nor in the expected type
    assert_eq!(
        run(
            "struct Box<T>\n    public v: T\nend\nimpl<T: Zero> Box<T>\n    public fun empty() -> Box<T>\n        Box { v: T.zero() }\n    end\nend\nlet b = Box.empty()\n"
        ),
        "COMPILE T171"
    );
    // a type without an impl does not satisfy the bound
    assert!(
        run("fun zero_of<T: Zero>() -> T\n    T.zero()\nend\nzero_of<string>()\n")
            .starts_with("COMPILE")
    );
    // `Self` outside a trait or an impl
    assert_eq!(run("fun f() -> Self\n    1\nend\n"), "COMPILE T001");
}
