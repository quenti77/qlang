//! `fun f(...xs: T)` takes the arguments left over; `...list` spreads a list in a call, an array
//! or a map.

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

const SUM: &str = "fun sum(...numbers: int) -> int
    let t = 0
    for n in numbers do
        t += n
    end
    t
end
";

#[test]
fn zero_one_and_several_arguments_are_collected() {
    let src = format!("{SUM}print(sum())\nprint(sum(4))\nprint(sum(1, 2, 3))\n");
    assert_eq!(run(&src), "0\n4\n6\n");
}

#[test]
fn fixed_parameters_come_first_and_keep_their_rules() {
    let src = "fun log(level: string, ...pieces: string) -> string
    level + \":\" + pieces.join(\"|\")
end
print(log(\"info\"))
print(log(\"info\", \"a\", \"b\"))
print(log(level: \"warn\"))
";
    assert_eq!(run(src), "info:\ninfo:a|b\nwarn:\n");
    assert_eq!(
        run("fun f(a: int, ...xs: int) -> int\n    a\nend\nprint(f())\n"),
        "COMPILE T240"
    );
}

#[test]
fn the_collected_array_is_new_and_has_the_right_type_when_empty() {
    let src = "fun grow(...xs: int) -> array<int>
    xs.push(9)
    xs
end
let a = [1, 2]
print(grow(...a))
print(a)
print(grow())
";
    assert_eq!(run(src), "[1, 2, 9]\n[1, 2]\n[9]\n");
}

#[test]
fn the_declared_type_is_the_type_of_one_element() {
    let src = "fun f(...notes: array<float>) -> int
    notes.len()
end
print(f([1.5], [2.5, 3.5]))
";
    assert_eq!(run(src), "2\n");
    // an `array<float>` argument is not a `float`
    assert_eq!(
        run("fun f(...notes: float) -> int\n    0\nend\nprint(f([1.5]))\n"),
        "COMPILE T002"
    );
}

#[test]
fn nullable_and_any_elements() {
    let src = "fun f(...xs: int?) -> int
    let n = 0
    for x in xs do
        if x != none then
            n += 1
        end
    end
    n
end
print(f(1, none, 3))
fun g(...xs: Any) -> int
    xs.len()
end
print(g(Any.of(1), Any.of(\"a\")))
";
    assert_eq!(run(src), "2\n2\n");
}

#[test]
fn a_spread_in_a_call_fills_the_fixed_parameters_then_the_variadic_one() {
    let src = format!(
        "{SUM}fun add(a: int, b: int, ...c: int) -> string
    \"{{a}} {{b}} {{c}}\"
end
let xs = [1, 2, 3]
print(sum(...xs))
print(sum(10, ...xs, 20))
print(sum(...xs, ...xs))
print(sum(...[]))
print(add(...[1, 2, 3, 4]))
print(add(...[1, 2]))
print(add(1, ...[2, 3]))
"
    );
    assert_eq!(run(&src), "6\n36\n12\n0\n1 2 [3, 4]\n1 2 []\n1 2 [3]\n");
}

#[test]
fn the_arity_after_a_spread_is_checked_when_the_call_runs() {
    let add = "fun add(a: int, b: int, ...c: int) -> int\n    a\nend\n";
    assert_eq!(run(&format!("{add}add(...[1])\n")), "RUNTIME R018");
    assert_eq!(run(&format!("{add}add(...[])\n")), "RUNTIME R018");
    let two = "fun two(a: int, b: int) -> int\n    a\nend\n";
    assert_eq!(run(&format!("{two}two(...[1, 2, 3])\n")), "RUNTIME R018");
    assert_eq!(run(&format!("{two}print(two(...[1, 2]))\n")), "1\n");
}

#[test]
fn a_default_before_the_variadic_parameter() {
    let src = "fun f(a: int = 1, ...xs: int) -> string
    \"{a} {xs}\"
end
print(f())
print(f(5))
print(f(5, 6, 7))
print(f(...[]))
print(f(...[1]))
print(f(...[1, 2, 3]))
";
    assert_eq!(run(src), "1 []\n5 []\n5 [6, 7]\n1 []\n1 []\n1 [2, 3]\n");
}

#[test]
fn spread_errors_are_reported_at_compile_time() {
    // an array of text does not fill an `int`
    assert_eq!(
        run(&format!("{SUM}print(sum(...[\"a\"]))\n")),
        "COMPILE T267"
    );
    assert_eq!(
        run("fun f(a: int, b: string) -> int\n    a\nend\nf(...[1, 2])\n"),
        "COMPILE T267"
    );
    // not an array
    assert_eq!(run(&format!("{SUM}print(sum(...3))\n")), "COMPILE T264");
    assert_eq!(
        run(&format!(
            "{SUM}let n: array<int>? = none\nprint(sum(...n))\n"
        )),
        "COMPILE T264"
    );
    assert_eq!(
        run(&format!("{SUM}let s = Set.of([1])\nprint(sum(...s))\n")),
        "COMPILE T264"
    );
    assert_eq!(run("let a = ...[1]\n"), "COMPILE P001");
    assert_eq!(run("print(1 + ...[1])\n"), "COMPILE P001");
}

#[test]
fn declaration_errors() {
    assert!(run("fun f(...a: int, b: int)\n    0\nend\n").contains("P044"));
    assert!(run("fun f(...a: int = 3)\n    0\nend\n").contains("P046"));
    assert_eq!(run("extern fun e(...a: int) -> int\n"), "COMPILE T261,T268");
}

#[test]
fn the_variadic_parameter_cannot_be_named() {
    let f = "fun f(a: int, ...xs: int) -> int\n    a\nend\n";
    assert_eq!(run(&format!("{f}f(1, xs: 2)\n")), "COMPILE T266");
    assert_eq!(run(&format!("{f}print(f(a: 7))\n")), "7\n");
    assert_eq!(run(&format!("{f}f(1, 2, a: 3)\n")), "COMPILE T266");
}

#[test]
fn generic_variadic_parameters_unify_all_the_arguments() {
    let src = "fun bind<T>(name: string, ...values: T) -> map<string, array<T>>
    { name: values }
end
print(bind(\"a\", 1, 2))
print(bind(\"b\", \"x\"))
";
    assert_eq!(run(src), "{\"a\": [1, 2]}\n{\"b\": [\"x\"]}\n");
    assert_eq!(
        run("fun bind<T>(...values: T) -> array<T>\n    values\nend\nbind(1, \"x\")\n"),
        "COMPILE T002"
    );
}

#[test]
fn methods_traits_and_overrides() {
    let src = "struct Stack
    public items: array<int>
end
impl Stack
    public fun push_all(self, ...xs: int)
        for x in xs do
            self.items.push(x)
        end
    end
end
trait Logger
    fun log(self, msg: string, ...extra: string) -> string
end
impl Logger for Stack
    public fun log(self, msg: string, ...extra: string) -> string
        msg + extra.join(\",\")
    end
end
let s = Stack { items: [] }
s.push_all(1, 2)
s.push_all(...[3, 4])
print(s.items)
print(s.log(\"a\", \"b\", \"c\"))
fun use_logger<L: Logger>(l: L) -> string
    l.log(\"x\", \"y\")
end
print(use_logger(s))
";
    assert_eq!(run(src), "[1, 2, 3, 4]\nab,c\nxy\n");
    // the impl must declare the same signature
    let bad = "trait L
    fun log(self, ...x: int) -> int
end
struct S
end
impl L for S
    public fun log(self, x: array<int>) -> int
        0
    end
end
";
    assert_eq!(run(bad), "COMPILE T134");
}

#[test]
fn function_types_and_anonymous_functions_can_be_variadic() {
    let src = "fun apply(f: fun(string, ...int) -> string) -> string
    f(\"x\", 1, 2, 3)
end
fun show(s: string, ...n: int) -> string
    \"{s} {n.len()}\"
end
print(apply(show))
let g = fun(...xs: int) -> int xs.len() end
print(g())
print(g(1, 2, 3))
let h: fun(...int) -> int = g
print(h(...[7, 8]))
";
    assert_eq!(run(src), "x 3\n0\n3\n2\n");
    // `fun(array<int>)` and `fun(...int)` are two different types
    let src = "let g = fun(...xs: int) -> int xs.len() end
let h: fun(array<int>) -> int = g
";
    assert_eq!(run(src), "COMPILE T002");
}

#[test]
fn recursion_with_a_spread() {
    let src = "fun total(...xs: int) -> int
    if xs.len() == 0 then
        return 0
    end
    xs[0] + total(...xs.slice(1, xs.len()))
end
print(total(1, 2, 3, 4))
";
    assert_eq!(run(src), "10\n");
}

#[test]
fn arrays_with_a_spread() {
    let src = "let a = [1, 2]
let b = [...a, 3, ...a]
print(b)
b.push(9)
print(a)
let empty: array<int> = []
print([...empty])
let copy = [...a]
copy.push(5)
print(a)
";
    assert_eq!(run(src), "[1, 2, 3, 1, 2]\n[1, 2]\n[]\n[1, 2]\n");
    assert_eq!(
        run("let a = [1, 2]\nlet c: array<float> = [...a]\n"),
        "COMPILE T002"
    );
    assert_eq!(run("let a = [1]\nlet b = [...a, \"x\"]\n"), "COMPILE T009");
    assert_eq!(
        run("let n: array<int>? = none\nlet b = [...n]\n"),
        "COMPILE T264"
    );
}

#[test]
fn maps_with_a_spread() {
    let src = "let defaults = { \"port\": 80, \"workers\": 2 }
let conf = { ...defaults, \"port\": 8080 }
print(conf)
print(defaults)
let both = { ...defaults, ...{ \"port\": 1, \"x\": 3 } }
print(both)
let none_at_all: map<string, int> = {}
print({ ...none_at_all })
";
    assert_eq!(
        run(src),
        "{\"port\": 8080, \"workers\": 2}\n{\"port\": 80, \"workers\": 2}\n{\"port\": 1, \"workers\": 2, \"x\": 3}\n{}\n"
    );
    assert_eq!(
        run("let a = { \"k\": 1 }\nlet b: map<string, float> = { ...a }\n"),
        "COMPILE T002"
    );
    assert_eq!(run("let a = [1]\nlet b = { ...a }\n"), "COMPILE T264");
}

#[test]
fn ranges_and_decimals_still_lex_as_before() {
    let src =
        "for i in 0..3 do\n    print(i)\nend\nfor j in 0..=2 do\n    print(j)\nend\nprint(1.5)\n";
    assert_eq!(run(src), "0\n1\n2\n0\n1\n2\n1.5\n");
}

#[test]
fn reflection_describes_a_variadic_parameter_by_its_element() {
    let src = "fun join_all(sep: string, ...pieces: string) -> string
    pieces.join(sep)
end
let c = Reflect.callable(\"join_all\")
if c != none then
    for p in c.params do
        print(\"{p.name} {p.variadic} {p.type_name}\")
    end
    print(c.call([Any.of(\"-\"), Any.of(\"a\"), Any.of(\"b\")]).unwrap().as<string>())
    print(c.call([]).unwrap_err().message)
end
";
    assert_eq!(
        run(src),
        "sep false string\npieces true string\na-b\nexpected at least 1 argument(s), found 0\n"
    );
}
