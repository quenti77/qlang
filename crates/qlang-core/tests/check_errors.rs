use qlang_core::check::compile;
use qlang_core::host::MemoryHost;

fn errors(src: &str) -> Vec<String> {
    let mut host = MemoryHost::new();
    host.allow_read = true;
    match compile("main.q", src, &mut host) {
        Ok(_) => vec![],
        Err(e) => e.diagnostics.iter().map(|d| format!("{}: {}", d.code, d.message)).collect(),
    }
}

fn expect(src: &str, needle: &str) {
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}\nsource:\n{src}\ngot: {errs:#?}"
    );
}

fn ok(src: &str) {
    let errs = errors(src);
    assert!(errs.is_empty(), "unexpected errors:\n{src}\n{errs:#?}");
}

#[test]
fn type_mismatch() {
    expect("let x: int = \"a\"", "expected `int`, found `string`");
    expect("let x = 1\nx = \"a\"", "expected `int`, found `string`");
    expect("let x = 1 + \"a\"", "cannot apply `+`");
    expect("if 1 then\nend", "must be a `bool`");
}

#[test]
fn unknown_names() {
    expect("print(y)", "unknown name `y`");
    expect("let x = Foo { a: 1 }", "unknown type `Foo`");
}

#[test]
fn const_and_init() {
    expect("const x = 1\nx = 2", "constant");
    expect("let x", "no initial value");
    expect("let x = none", "cannot infer");
    expect("let x: int = none", "expected `int`, found `none`");
    ok("let x: int? = none");
}

#[test]
fn nullable_flow() {
    expect("let x: int? = 1\nlet y = x + 1", "may be `none`");
    ok("let x: int? = 1\nif x != none then\nlet y = x + 1\nend");
    ok("fun f(x: int?) -> int\nif x == none then\nreturn 0\nend\nx + 1\nend");
    expect("let x: int? = 1\nlet y: int = x", "may be `none`");
}

#[test]
fn functions() {
    expect("fun f(a: int) -> int\na\nend\nf(1, 2)", "expects 1 argument");
    expect("fun f(a: int) -> int\nend", "must give a value");
    expect("fun f(a: int)\nreturn 1\nend", "does not return a value");
    ok("fun f(a: int) -> int\nif a > 0 then\nreturn 1\nelse\nreturn 2\nend\nend");
}

#[test]
fn mixed_numbers_work() {
    ok("let a = 1 + 2.5\nlet b: float = 7 / 2\nlet c: int = 7 div 2\nlet d: int = 7 mod 2");
    expect("let c: int = 7 / 2", "expected `int`, found `float`");
    expect("let c = 7.5 div 2", "cannot apply `div`");
}

#[test]
fn generics() {
    ok("fun id<T>(x: T) -> T\nx\nend\nlet a: int = id(1)\nlet b: string = id(\"s\")");
    expect("fun add<T>(a: T, b: T) -> T\na + b\nend", "cannot apply `+`");
    ok("fun add<T: Add>(a: T, b: T) -> T\na + b\nend\nlet x = add(1, 2)");
    expect("fun add<T: Add>(a: T, b: T) -> T\na + b\nend\nlet x = add(true, false)", "does not implement");
}

#[test]
fn structs() {
    expect("struct P\npublic x: int\nend\nlet p = P { y: 1 }", "no field `y`");
    expect("struct P\npublic x: int\nend\nlet p = P { }", "missing field");
    expect("struct P\nprivate x: int\nend\nlet p = P { x: 1 }", "private");
    ok("struct P\npublic x: int\nend\nimpl P\npublic fun get(self) -> int\nself.x\nend\nend\nlet p = P { x: 1 }\nprint(p.get())");
    expect("struct P\nprivate x: int\nend\nimpl P\npublic fun new() -> P\nP { x: 1 }\nend\nend\nlet p = P.new()\nprint(p.x)", "private");
}

#[test]
fn inheritance() {
    expect(
        "struct A\nend\nstruct B extends A\nend\nimpl A\npublic fun f(self) -> int\n1\nend\nend\nimpl B\npublic fun f(self) -> int\n2\nend\nend",
        "add `override`",
    );
    expect(
        "struct A\nend\nstruct B extends A\nend\nimpl B\npublic override fun f(self) -> int\n2\nend\nend",
        "no parent struct has this method",
    );
}

#[test]
fn traits_and_ops() {
    expect("struct P\npublic x: int\nend\nlet p = P { x: 1 } + P { x: 2 }", "cannot apply `+`");
    expect("trait T\nfun f(self) -> int\nend\nstruct S\nend\nimpl T for S\nend", "missing method `f`");
}

#[test]
fn control_flow() {
    expect("break", "only allowed inside a loop");
    expect("for i in 5 do\nend", "cannot loop over");
    ok("for c in \"abc\" do\nprint(c)\nend");
}

#[test]
fn read_capability() {
    let mut host = MemoryHost::new();
    host.allow_read = false;
    let r = compile("main.q", "let x = read()", &mut host);
    let msgs: Vec<String> = match r {
        Ok(_) => vec![],
        Err(e) => e.diagnostics.iter().map(|d| d.message.clone()).collect(),
    };
    assert!(msgs.iter().any(|m| m.contains("not available")), "{msgs:?}");
}

#[test]
fn parse_errors_are_reported() {
    expect("let x = (1 + ", "expected");
    expect("let x = 1 <\n", "expected");
    expect("let x = 1 @ 2", "unexpected character");
    expect("print(\"abc", "unterminated string");
}
