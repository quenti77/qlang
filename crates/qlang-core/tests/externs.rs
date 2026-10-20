//! `extern fun`: functions the host provides, with plain data copied across.

use qlang_core::check::compile;
use qlang_core::host::{HostError, HostValue, Limits, MemoryHost};

fn host_with(
    name: &str,
    f: impl FnMut(Vec<HostValue>) -> Result<HostValue, HostError> + Send + 'static,
) -> MemoryHost {
    let mut host = MemoryHost::default();
    host.externs.insert(name.to_string(), Box::new(f));
    host
}

/// Output of the program, or `CODE: message` (compile or run error).
fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
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
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn s(x: &str) -> HostValue {
    HostValue::Str(x.to_string())
}

#[test]
fn a_host_function_is_called_with_plain_values() {
    let host = host_with("add", |args| match args.as_slice() {
        [HostValue::Int(a), HostValue::Int(b)] => Ok(HostValue::Int(a + b)),
        other => Err(HostError::Other(format!("bad args {other:?}"))),
    });
    let out = exec(
        "extern fun add(a: int, b: int) -> int\nprint(add(2, 40))",
        host,
    );
    assert_eq!(out, "42\n");
}

#[test]
fn a_missing_host_function_is_a_compile_error() {
    let out = exec(
        "extern fun add(a: int, b: int) -> int\nprint(add(2, 40))",
        MemoryHost::default(),
    );
    assert!(out.starts_with("COMPILE: T263"), "{out}");
}

#[test]
fn declaring_a_missing_function_without_using_it_is_fine() {
    let out = exec(
        "extern fun add(a: int, b: int) -> int\nprint(1)",
        MemoryHost::default(),
    );
    assert_eq!(out, "1\n");
}

#[test]
fn a_failure_is_an_err_when_the_signature_returns_a_result() {
    let host = host_with("open", |_| Err(HostError::Other("no such db".into())));
    let out = exec(
        "extern fun open(path: string) -> Result<int, Error>\nlet r = open(\"x\")\nprint(r.is_err())\nprint(r.unwrap_err().message)",
        host,
    );
    assert_eq!(out, "true\nno such db\n");
}

#[test]
fn a_result_signature_wraps_success_too() {
    let host = host_with("open", |_| Ok(HostValue::Int(7)));
    let out = exec(
        "extern fun open(path: string) -> Result<int, Error>\nprint(open(\"x\").unwrap())",
        host,
    );
    assert_eq!(out, "7\n");
}

#[test]
fn a_failure_stops_the_program_without_a_result() {
    let host = host_with("boom", |_| Err(HostError::Other("it broke".into())));
    let out = exec("extern fun boom() -> int\nprint(boom())", host);
    assert_eq!(out, "R081: `boom` failed: it broke");
}

#[test]
fn structs_cross_as_maps_and_enums_as_names() {
    let host = host_with("echo", |args| {
        assert_eq!(
            args,
            vec![HostValue::Map(vec![
                (s("name"), s("Rex")),
                (s("age"), HostValue::Int(3)),
                (s("mood"), s("Happy")),
                (s("tags"), HostValue::Array(vec![s("a")])),
                (s("nick"), HostValue::Null),
            ])]
        );
        Ok(args.into_iter().next().unwrap())
    });
    let src = r#"
enum Mood
  Happy
  Sad
end
struct Dog
  public name: string
  public age: int
  public mood: Mood
  public tags: array<string>
  public nick: string?
end
extern fun echo(d: Dog) -> Dog
let d = echo(Dog { name: "Rex", age: 3, mood: Mood.Happy, tags: ["a"], nick: none })
print("{d.name} {d.age} {d.mood == Mood.Happy} {d.tags.len()} {d.nick == none}")
"#;
    assert_eq!(exec(src, host), "Rex 3 true 1 true\n");
}

#[test]
fn json_crosses_as_json_shaped_data() {
    let host = host_with("parse", |_| {
        Ok(HostValue::Map(vec![
            (s("n"), HostValue::Int(2)),
            (
                s("xs"),
                HostValue::Array(vec![HostValue::Bool(true), HostValue::Null]),
            ),
        ]))
    });
    let out = exec(
        r#"extern fun parse(text: string) -> Json
let j = parse("whatever")
print(j.is_object())
let n = j.get("n")
let xs = j.get("xs")
if n != none then print(n.as_number()) end
if xs != none then print(xs.as_array().len()) end"#,
        host,
    );
    assert_eq!(out, "true\n2.0\n2\n");
}

#[test]
fn a_wrong_answer_from_the_host_is_a_runtime_error() {
    let host = host_with("n", |_| Ok(HostValue::Str("oops".into())));
    let out = exec("extern fun n() -> int\nprint(n())", host);
    assert!(out.starts_with("R083"), "{out}");
}

#[test]
fn types_that_cannot_cross_are_refused() {
    let host = MemoryHost::default();
    let out = exec("extern fun f(x: fun(int) -> int) -> int", host);
    assert!(out.starts_with("COMPILE: T261"), "{out}");
    let out = exec("extern fun f<T>(x: int) -> int", MemoryHost::default());
    assert!(out.contains("T260"), "{out}");
    let out = exec(
        "struct N\n  next: N?\nend\nextern fun f() -> N",
        MemoryHost::default(),
    );
    assert!(out.contains("T262") && out.contains("recursive"), "{out}");
}

#[test]
fn extern_works_across_modules() {
    let mut host = host_with("twice", |a| match a.as_slice() {
        [HostValue::Int(x)] => Ok(HostValue::Int(x * 2)),
        _ => Err(HostError::Other("bad".into())),
    });
    host.files.insert(
        "lib.q".into(),
        "extern fun twice(x: int) -> int\nexport twice".into(),
    );
    let out = exec("import twice from \"lib.q\"\nprint(twice(21))", host);
    assert_eq!(out, "42\n");
}
