//! `Reflect`: structs, enums and the functions of a file, with their annotations.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => host.output,
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            codes.join(",")
        }
    }
}

const DECLS: &str = "#[annotation]
struct Route
  path: string
  method: string = \"GET\"
  weight: float = 1.5
  tags: array<string> = []
end

#[annotation]
struct Min
  value: int
end

";

fn with(body: &str) -> String {
    format!("{DECLS}{body}")
}

#[test]
fn describes_a_struct_with_its_annotations() {
    let src = with(
        "#[Route(\"/form\", tags: [\"a\", \"b\"])]
struct Form
  #[Min(-3)]
  age: int
  name: string?
  items: array<int>
end

let s = Reflect.struct_info<Form>()
print(s.name)
for f in s.fields do
  print(\"{f.name}: {f.type_name} ({f.annotations.len()})\")
end
let r = s.annotation(\"Route\")
if r != none then
  print(r.name)
  print(r.args.len())
  let p = r.args[\"path\"]
  let m = r.args[\"method\"]
  let w = r.args[\"weight\"]
  let t = r.args[\"tags\"]
  if p != none and m != none and w != none and t != none then
    print(\"{p.as_string()} {m.as_string()} {w.as_number()} {t.as_array().len()}\")
  end
end
let age = s.fields[0].annotation(\"Min\")
if age != none then
  let v = age.args[\"value\"]
  if v != none then
    print(v.as_number())
  end
end
print(s.annotation(\"Nope\") == none)
",
    );
    assert_eq!(
        run(&src),
        "Form\nage: int (1)\nname: string? (0)\nitems: array<int> (0)\nRoute\n4\n/form GET 1.5 2\n-3.0\ntrue\n"
    );
}

#[test]
fn describes_an_enum_and_functions() {
    let src = with(
        "enum Shape
  #[Min(1)]
  Dot
  Rect(w: int, h: float)
end

#[Route(\"/a\")]
fun a(#[Min(2)] x: int, y: string) -> bool
  true
end

fun b()
end

let e = Reflect.enum_info<Shape>()
print(e.name)
for v in e.variants do
  print(\"{v.name} {v.fields.len()} {v.annotations.len()}\")
end
print(e.variants[1].fields[1].type_name)
for f in Reflect.functions() do
  print(\"{f.name}({f.params.len()}) -> {f.return_type} [{f.annotations.len()}]\")
end
let first = Reflect.functions()[0]
print(first.params[0].name)
print(first.params[0].annotations.len())
print(first.params[1].type_name)
",
    );
    assert_eq!(
        run(&src),
        "Shape\nDot 0 1\nRect 2 0\nfloat\na(2) -> bool [1]\nb(0) -> no value [0]\nx\n1\nstring\n"
    );
}

#[test]
fn finds_a_function_by_name_and_type() {
    let src = with(
        "fun double(x: int) -> int
  x * 2
end

fun greet(name: string) -> string
  \"hi {name}\"
end

fun generic<T>(x: T) -> T
  x
end

let f = Reflect.find<fun(int) -> int>(\"double\")
if f != none then
  print(f(21))
end
let g = Reflect.find<fun(string) -> string>(\"greet\")
if g != none then
  print(g(\"you\"))
end
print(Reflect.find<fun(int) -> int>(\"greet\") == none)
print(Reflect.find<fun(int) -> int>(\"missing\") == none)
print(Reflect.find<fun(int) -> int>(\"generic\") == none)
",
    );
    assert_eq!(run(&src), "42\nhi you\ntrue\ntrue\ntrue\n");
}

#[test]
fn mistakes_are_reported() {
    assert_eq!(
        run("struct S\nend\nlet x = Reflect.struct_info<int>()\n"),
        "T301"
    );
    assert_eq!(run("let x = Reflect.struct_info()\n"), "T300");
    assert_eq!(run("let x = Reflect.enum_info<Nope>()\n"), "T001");
    assert_eq!(run("let x = Reflect.find<int>(\"a\")\n"), "T301");
    assert_eq!(run("let x = Reflect.nothing()\n"), "T302");
}
