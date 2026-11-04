//! A type of the program that implements `Iterable<T>` / `KeyedIterable<K, V>` goes through
//! `for` and `...`; an `Iterator<T>` has lazy `map`, `filter`, `take`, `skip`.

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

const COUNT: &str = "struct Count
    public n: int
    public max: int
end
impl Iterator<int> for Count
    public fun next(self) -> Step<int>
        if self.n >= self.max then
            return Step<int>.End
        end
        self.n += 1
        Step<int>.Item(self.n)
    end
end
impl Iterable<int> for Count
    public fun iter(self) -> Iterator<int>
        Count { n: 0, max: self.max }
    end
end
";

const PAIRS: &str = "struct Pairs
    public last: int
end
impl Iterator<(string, int)> for Pairs
    public fun next(self) -> Step<(string, int)>
        Step<(string, int)>.End
    end
end
impl KeyedIterable<string, int> for Pairs
    public fun iter_keyed(self) -> Iterator<(string, int)>
        ArrayIter<(string, int)> { items: [(\"a\", 1), (\"b\", 2)], at: 0 }
    end
end
";

#[test]
fn for_goes_through_an_iterable() {
    let src = format!("{COUNT}for x in Count {{ n: 0, max: 3 }} do print(x) end\n");
    assert_eq!(run(&src), "1\n2\n3\n");
}

#[test]
fn two_variables_give_a_counter_then_the_element() {
    let src =
        format!("{COUNT}for i, x in Count {{ n: 0, max: 2 }} do print(\"{{i}}:{{x}}\") end\n");
    assert_eq!(run(&src), "0:1\n1:2\n");
}

#[test]
fn a_keyed_iterable_gives_key_and_value() {
    let src = format!("{PAIRS}for k, v in Pairs {{ last: 0 }} do print(\"{{k}}={{v}}\") end\n");
    assert_eq!(run(&src), "a=1\nb=2\n");
}

#[test]
fn break_stops_the_loop() {
    let src = format!(
        "{COUNT}for x in Count {{ n: 0, max: 9 }} do
    if x == 3 then break end
    print(x)
end
"
    );
    assert_eq!(run(&src), "1\n2\n");
}

#[test]
fn an_element_that_is_none_does_not_end_the_loop() {
    let src = "struct Holes
    public at: int
end
impl Iterator<int?> for Holes
    public fun next(self) -> Step<int?>
        if self.at >= 3 then
            return Step<int?>.End
        end
        self.at += 1
        Step<int?>.Item(none)
    end
end
let n = 0
for x in Holes { at: 0 } do n += 1 end
print(n)
";
    assert_eq!(run(src), "3\n");
}

#[test]
fn a_type_parameter_bounded_by_iterable_can_be_looped_over() {
    let src = format!(
        "{COUNT}fun total<I: Iterable<int>>(c: I) -> int
    let s = 0
    for x in c do s += x end
    s
end
print(total(Count {{ n: 0, max: 4 }}))
print(total([5, 6]))
"
    );
    assert_eq!(run(&src), "10\n11\n");
}

#[test]
fn spread_takes_an_iterable_in_an_array_a_call_and_a_map() {
    let src = format!(
        "{COUNT}{PAIRS}fun add3(a: int, b: int, c: int) -> int
    a + b + c
end
print([...Count {{ n: 0, max: 2 }}, 9])
print(add3(...Count {{ n: 0, max: 3 }}))
print({{ ...Pairs {{ last: 0 }}, \"z\": 0 }})
"
    );
    assert_eq!(run(&src), "[1, 2, 9]\n6\n{\"a\": 1, \"b\": 2, \"z\": 0}\n");
}

#[test]
fn adapters_are_lazy_and_compose() {
    let src = format!(
        "{COUNT}let it = Count {{ n: 0, max: 1000000000 }}.filter(fun(x: int) -> bool x % 2 == 0 end).map(fun(x: int) -> int x * x end).skip(1).take(3)
print(it.collect())
print(Count {{ n: 0, max: 5 }}.count())
print(Count {{ n: 0, max: 0 }}.first())
"
    );
    assert_eq!(run(&src), "[16, 36, 64]\n5\nnone\n");
}

#[test]
fn builtin_collections_have_an_iterator() {
    let src = "print([1, 2, 3].iter().map(fun(x: int) -> int x + 1 end).collect())
print(\"ab\".iter().collect())
print({ \"a\": 1 }.iter_keyed().collect())
";
    assert_eq!(run(src), "[2, 3, 4]\n[\"a\", \"b\"]\n[(\"a\", 1)]\n");
}

#[test]
fn a_type_that_is_not_iterable_is_refused() {
    let src = "struct P\n    public x: int\nend\nfor x in P { x: 1 } do print(x) end\n";
    assert_eq!(run(src), "COMPILE T190");
}

#[test]
fn a_map_still_needs_two_variables() {
    assert_eq!(
        run("for x in { \"a\": 1 } do print(x) end\n"),
        "COMPILE T190"
    );
}
