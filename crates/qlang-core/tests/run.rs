//! End-to-end tests: compile and run small programs.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

struct Outcome {
    output: String,
    error: Option<String>,
}

fn run_files(files: &[(&str, &str)], input: Option<&[&str]>, limits: Limits) -> Outcome {
    let files: Vec<(String, String)> = files.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    let input: Option<Vec<String>> = input.map(|i| i.iter().map(|s| s.to_string()).collect());
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            let mut host = MemoryHost::new();
            for (n, t) in &files {
                host.files.insert(n.clone(), t.clone());
            }
            if let Some(i) = input {
                host.allow_read = true;
                host.input = i.into();
            }
            let main = files[0].1.clone();
            let prog = match compile(&files[0].0, &main, &mut host) {
                Ok(p) => p,
                Err(e) => return Outcome { output: String::new(), error: Some(format!("COMPILE: {}", e.render())) },
            };
            let r = prog.run(&mut host, limits);
            Outcome { output: host.output, error: r.error.map(|d| format!("{}: {}", d.code, d.message)) }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn run(src: &str) -> Outcome {
    run_files(&[("main.q", src)], None, Limits::default())
}

fn out(src: &str) -> String {
    let o = run(src);
    if let Some(e) = o.error {
        panic!("unexpected error: {e}\noutput so far:\n{}", o.output);
    }
    o.output
}

fn err(src: &str) -> String {
    let o = run(src);
    o.error.unwrap_or_else(|| panic!("expected an error, output was:\n{}", o.output))
}

#[test]
fn arithmetic() {
    assert_eq!(out("print(7 / 2)\nprint(7 div 2)\nprint(7 mod 2)\nprint(-7 div 2)\nprint(-7 mod 2)"), "3.5\n3\n1\n-3\n-1\n");
    assert_eq!(out("print(2 ** 3 ** 2)\nprint(-2 ** 2)\nprint(2.0 ** 0.5 > 1.4)"), "512\n-4\ntrue\n");
    assert_eq!(out("print(1 + 2 * 3)\nprint((1 + 2) * 3)\nprint(10 - 4 - 3)"), "7\n9\n3\n");
    assert_eq!(out("print(1 + 2.5)\nprint(3 * 1.5)\nprint(10 / 4)\nprint(4 / 2)"), "3.5\n4.5\n2.5\n2.0\n");
    assert_eq!(out("print(5.5 mod 2)\nprint(1_000 + 0xFF + 0b11)"), "1.5\n1258\n");
    assert_eq!(out("let x = 5\nx += 2\nx *= 3\nx -= 1\nprint(x)\nlet y = 5.0\ny /= 4\nprint(y)"), "20\n1.25\n");
}

#[test]
fn arithmetic_errors() {
    assert!(err("print(1 / 0)").starts_with("R011"));
    assert!(err("print(1 div 0)").starts_with("R011"));
    assert!(err("print(1 mod 0)").starts_with("R011"));
    assert!(err("print(9223372036854775807 + 1)").starts_with("R010"));
    assert!(err("print(2 ** 64)").starts_with("R010"));
    assert!(err("print(2 ** -1)").starts_with("R012"));
}

#[test]
fn comparisons_and_logic() {
    assert_eq!(out("print(1 < 2)\nprint(2 <= 2)\nprint(1 == 1.0)\nprint(\"a\" < \"b\")\nprint(1 != 2)"), "true\ntrue\ntrue\ntrue\ntrue\n");
    assert_eq!(out("print(true and false)\nprint(true or false)\nprint(not true)"), "false\ntrue\nfalse\n");
    // short-circuit: the right side must not run
    assert_eq!(out("fun boom() -> bool\npanic(\"no\")\nend\nprint(false and boom())\nprint(true or boom())"), "false\ntrue\n");
}

#[test]
fn strings() {
    assert_eq!(out("let n = 3\nprint(\"n = {n}, next = {n + 1}\")"), "n = 3, next = 4\n");
    assert_eq!(out("print(\"a\\tb\\\\c\\\"d\\{e\\}\")"), "a\tb\\c\"d{e}\n");
    assert_eq!(out("let s = \"Hello\"\nprint(s.upper())\nprint(s.len())\nprint(s[1])\nprint(s.substring(1, 3))"), "HELLO\n5\ne\nel\n");
    assert_eq!(out("print(\"a,b,c\".split(\",\"))\nprint(\"x\".repeat(3))\nprint(\" hi \".trim() + \"!\")"), "[\"a\", \"b\", \"c\"]\nxxx\nhi!\n");
    assert_eq!(out("print(\"héllo\".len())\nprint(\"héllo\".index_of(\"l\"))"), "5\n2\n");
    assert_eq!(out("print(\"ab\\ncd\")\nprint(\"line1\nline2\")"), "ab\ncd\nline1\nline2\n");
    assert_eq!(out("for c in \"abc\" do\nwrite(c)\nwrite(\"-\")\nend\nprint(\"\")"), "a-b-c-\n");
}

#[test]
fn variables_and_const() {
    assert_eq!(out("const t = [0]\nt[0] = 2\nprint(t)"), "[2]\n");
    assert_eq!(out("const t = [0]\nlet u = t\nu[0] = 9\nprint(t[0])"), "9\n");
    assert!(err("const t = 1\nt = 2").starts_with("COMPILE"));
}

#[test]
fn control_flow() {
    assert_eq!(out("for i in 0..3 do\nprint(i)\nend"), "0\n1\n2\n");
    assert_eq!(out("for i in 0..=3 do\nwrite(i)\nend\nprint(\"\")"), "0123\n");
    assert_eq!(out("for i in 10..0 step -3 do\nwrite(\"{i} \")\nend\nprint(\"\")"), "10 7 4 1 \n");
    assert_eq!(out("for i in 5..1 do\nprint(i)\nend\nprint(\"none\")"), "none\n");
    assert_eq!(out("let i = 0\nwhile true do\ni += 1\nif i == 2 then continue end\nif i > 4 then break end\nwrite(i)\nend\nprint(\"\")"), "134\n");
    assert_eq!(out("let x = 5\nif x < 3 then print(\"a\") elseif x < 10 then print(\"b\") else print(\"c\") end"), "b\n");
    assert_eq!(out("let v = if 1 > 2 then \"x\" else \"y\" end\nprint(v)"), "y\n");
    assert_eq!(out("for x in [10, 20] do\nprint(x)\nend"), "10\n20\n");
    assert!(err("for i in 0..3 step 0 do\nend").starts_with("R031"));
}

#[test]
fn match_expressions() {
    let src = "fun f(n: int) -> string\nmatch n\ncase 0 then \"zero\"\ncase 1..=9 then \"small\"\ncase 10..20 then \"teen\"\ncase x if x < 0 then \"neg\"\nelse \"big\"\nend\nend\n";
    assert_eq!(out(&format!("{src}print(f(0))\nprint(f(5))\nprint(f(10))\nprint(f(20))\nprint(f(-4))")), "zero\nsmall\nteen\nbig\nneg\n");
    assert_eq!(
        out("enum C\nA\nB\nend\nlet c = C.B\nprint(match c\ncase C.A then 1\ncase C.B then 2\nend)\nprint(C.A == C.A)\nprint(C.A != C.B)\nprint(c)"),
        "2\ntrue\ntrue\nC.B\n"
    );
    assert_eq!(out("let s: string? = none\nprint(match s\ncase none then \"empty\"\ncase v then v.upper()\nend)"), "empty\n");
    assert_eq!(out("print(match \"hi\"\ncase \"hi\" then 1\ncase \"yo\" then 2\nelse 3\nend)"), "1\n");
}

#[test]
fn nullables() {
    assert_eq!(out("let x: int? = none\nprint(x)\nx = 4\nprint(x)\nprint(x == 4)\nprint(x == none)"), "none\n4\ntrue\nfalse\n");
    assert_eq!(out("fun f(x: int?) -> int\nif x == none then\nreturn -1\nend\nx * 2\nend\nprint(f(none))\nprint(f(4))"), "-1\n8\n");
    assert_eq!(out("let a: int? = 3\nif a != none and a > 2 then print(\"big\") end"), "big\n");
    assert_eq!(out("print(int.parse(\"12\"))\nprint(int.parse(\"x\"))\nprint(float.parse(\"2.5\"))"), "12\nnone\n2.5\n");
    assert_eq!(out("let v = [1, 2, 3]\nprint(v.pop())\nlet e: array<int> = []\nprint(e.pop())"), "3\nnone\n");
}

#[test]
fn functions_and_closures() {
    assert_eq!(out("fun fib(n: int) -> int\nif n < 2 then return n end\nfib(n - 1) + fib(n - 2)\nend\nprint(fib(15))"), "610\n");
    assert_eq!(
        out("fun make() -> fun() -> int\nlet n = 0\nfun() -> int\nn += 1\nn\nend\nend\nlet a = make()\nlet b = make()\na()\na()\nprint(a())\nprint(b())"),
        "3\n1\n"
    );
    assert_eq!(out("let fs: array<fun() -> int> = []\nfor i in 0..3 do\nfs[] = fun() -> int\ni * 10\nend\nend\nfor f in fs do\nwrite(\"{f()} \")\nend\nprint(\"\")"), "0 10 20 \n");
    assert_eq!(out("fun twice(f: fun(int) -> int, x: int) -> int\nf(f(x))\nend\nprint(twice(fun(n: int) -> int\nn + 3\nend, 1))"), "7\n");
    assert_eq!(out("fun noop()\nend\nnoop()\nprint(\"ok\")"), "ok\n");
}

#[test]
fn generics() {
    assert_eq!(out("fun id<T>(x: T) -> T\nx\nend\nprint(id(5))\nprint(id(\"s\"))\nprint(id<float>(1.5))"), "5\ns\n1.5\n");
    assert_eq!(out("fun big<T: Ord>(a: T, b: T) -> T\nif a > b then a else b end\nend\nprint(big(2, 7))\nprint(big(2.5, 1.5))"), "7\n2.5\n");
    assert_eq!(
        out("struct Box<T>\npublic v: T\nend\nimpl<T> Box<T>\npublic fun new(v: T) -> Box<T>\nBox { v }\nend\npublic fun get(self) -> T\nself.v\nend\nend\nlet b = Box.new(5)\nprint(b.get() + 1)\nlet c = Box<string>.new(\"x\")\nprint(c.get() + \"y\")"),
        "6\nxy\n"
    );
    assert_eq!(out("fun sum<T: Add>(a: T, b: T) -> T\na + b\nend\nprint(sum(1, 2))\nprint(sum(\"a\", \"b\"))\nprint(sum(1.5, 2.5))"), "3\nab\n4.0\n");
}

#[test]
fn arrays() {
    assert_eq!(out("let a = [1, 2]\na[] = 3\na.push(4)\nprint(a)\nprint(a.len())\na[0] += 10\nprint(a[0])"), "[1, 2, 3, 4]\n4\n11\n");
    assert_eq!(out("let a = [3, 1, 2]\na.reverse()\nprint(a)\nprint(a.contains(1))\nprint(a.index_of(9))\nprint(a.remove(0))\na.insert(0, 7)\nprint(a)"), "[2, 1, 3]\ntrue\nnone\n2\n[7, 1, 3]\n");
    assert_eq!(out("let a = [[1, 2], [3]]\nprint(a[0][1])\nprint(a)"), "2\n[[1, 2], [3]]\n");
    assert_eq!(out("let names = [\"b\", \"a\"]\nprint(names.join(\"+\"))\nprint([1, 2] == [1, 2])"), "b+a\ntrue\n");
    assert!(err("let a = [1]\nprint(a[5])").starts_with("R020"));
    assert!(err("let a = [1]\nprint(a[-1])").starts_with("R020"));
    assert_eq!(out("let a: array<float> = []\na[] = 1.5\nprint(a)"), "[1.5]\n");
}

#[test]
fn structs_and_methods() {
    let src = "struct P\npublic x: int\npublic y: int = 7\nend\nimpl P\npublic fun new(x: int) -> P\nP { x }\nend\npublic fun sum(self) -> int\nself.x + self.y\nend\npublic fun bump(self)\nself.x += 1\nend\nend\n";
    assert_eq!(out(&format!("{src}let p = P.new(1)\np.bump()\nprint(p.sum())\nlet q = p\nq.bump()\nprint(p.x)\nprint(p)")), "9\n3\nP { x: 3, y: 7 }\n");
    assert_eq!(out(&format!("{src}let a = P {{ x: 1 }}\nlet b = P {{ ..a, y: 0 }}\nb.x = 50\nprint(a.x)\nprint(b)")), "1\nP { x: 50, y: 0 }\n");
    assert_eq!(out(&format!("{src}let p = P {{ x: 1, y: 2 }}\nlet x = 9\nlet r = P {{ x, y: 3 }}\nprint(r.x)")), "9\n");
    assert_eq!(out("struct C\npublic cb: fun(int) -> int\nend\nlet c = C { cb: fun(n: int) -> int\nn * 2\nend }\nprint(c.cb(4))"), "8\n");
}

#[test]
fn statics_and_privacy() {
    let src = "struct K\nprivate static n: int = 0\npublic id: int\nend\nimpl K\npublic fun new() -> K\nK.n += 1\nK { id: K.n }\nend\npublic fun count() -> int\nK.n\nend\nend\n";
    assert_eq!(out(&format!("{src}let a = K.new()\nlet b = K.new()\nprint(b.id)\nprint(K.count())")), "2\n2\n");
    assert!(err(&format!("{src}print(K.n)")).starts_with("COMPILE"));
}

#[test]
fn inheritance() {
    let src = "struct A\nprotected v: int = 1\nend\nstruct B extends A\npublic w: int = 2\nend\nimpl A\npublic fun name(self) -> string\n\"A{self.v}\"\nend\npublic fun id() -> int\n10\nend\nend\nimpl B\npublic override fun name(self) -> string\nsuper.name() + \"B{self.w}\"\nend\npublic fun total(self) -> int\nself.v + self.w\nend\nend\n";
    assert_eq!(out(&format!("{src}let b = B {{ }}\nprint(b.name())\nprint(b.total())\nprint(B.id())")), "A1B2\n3\n10\n");
    // a B is usable where an A is expected, and overrides still apply
    assert_eq!(out(&format!("{src}fun show(a: A) -> string\na.name()\nend\nprint(show(B {{ }}))\nprint(show(A {{ }}))")), "A1B2\nA1\n");
    assert!(err(&format!("{src}let b = B {{ }}\nprint(b.v)")).starts_with("COMPILE"));
}

#[test]
fn traits_and_operators() {
    let pt = "struct V\npublic x: int\nend\nimpl Add for V\npublic fun add(self, o: V) -> V\nV { x: self.x + o.x }\nend\nend\nimpl Sub for V\npublic fun sub(self, o: V) -> V\nV { x: self.x - o.x }\nend\nend\nimpl Neg for V\npublic fun neg(self) -> V\nV { x: -self.x }\nend\nend\nimpl Ord for V\npublic fun cmp(self, o: V) -> int\nself.x - o.x\nend\nend\nimpl Eq for V\npublic fun eq(self, o: V) -> bool\nself.x == o.x\nend\nend\n";
    assert_eq!(out(&format!("{pt}let a = V {{ x: 5 }}\nlet b = V {{ x: 2 }}\nprint((a + b).x)\nprint((a - b).x)\nprint((-a).x)\nprint(a > b)\nprint(a == b)\nprint(a != b)")), "7\n3\n-5\ntrue\nfalse\ntrue\n");
    // mixed numeric operators defined by the user, on a builtin left operand
    assert_eq!(out("struct V\npublic x: int\nend\nimpl Mul<V> for int\npublic fun mul(self, v: V) -> V\nV { x: self * v.x }\nend\nend\nimpl Mul<int> for V\npublic fun mul(self, k: int) -> int\nself.x * k\nend\nend\nprint((3 * V { x: 4 }).x)\nprint(V { x: 4 } * 5)"), "12\n20\n");
    // operator on a generic bound resolves at runtime
    assert_eq!(out(&format!("{pt}fun total<T: Add>(a: T, b: T) -> T\na + b\nend\nprint(total(V {{ x: 1 }}, V {{ x: 2 }}).x)")), "3\n");
}

#[test]
fn index_and_push_traits() {
    let src = "struct Bag\npublic items: array<int>\nend\nimpl Index<int> for Bag\npublic fun index(self, i: int) -> int\nself.items[i] * 10\nend\nend\nimpl IndexSet<int, int> for Bag\npublic fun set_index(self, i: int, v: int)\nself.items[i] = v\nend\nend\nimpl Push<int> for Bag\npublic fun push(self, v: int)\nself.items[] = v\nend\nend\n";
    assert_eq!(out(&format!("{src}let b = Bag {{ items: [1, 2] }}\nb[] = 3\nb[0] = 5\nb[1] += 1\nprint(b[0])\nprint(b[1])\nprint(b[2])\nprint(b.items)")), "50\n210\n30\n[5, 21, 3]\n");
}

#[test]
fn conversions() {
    assert_eq!(out("print(3 as float)\nprint(3.99 as int)\nprint(-3.99 as int)\nprint(42 as string + \"!\")\nprint(true as string)"), "3.0\n3\n-3\n42!\ntrue\n");
    assert_eq!(out("struct P\npublic n: int\nend\nimpl As<string> for P\npublic fun convert(self) -> string\n\"P#{self.n}\"\nend\nend\nimpl As<int> for P\npublic fun convert(self) -> int\nself.n\nend\nend\nlet p = P { n: 4 }\nprint(p as string)\nprint(p as int + 1)\nprint(p)\nprint(\"v={p}\")\nprint([p])"), "P#4\n5\nP#4\nv=P#4\n[P#4]\n");
}

#[test]
fn traits_as_types() {
    let src = "trait Speak\nfun say(self) -> string\nend\nstruct Dog\nend\nstruct Cat\nend\nimpl Speak for Dog\npublic fun say(self) -> string\n\"woof\"\nend\nend\nimpl Speak for Cat\npublic fun say(self) -> string\n\"meow\"\nend\nend\n";
    assert_eq!(out(&format!("{src}let pets: array<Speak> = [Dog {{ }}, Cat {{ }}]\nfor p in pets do\nprint(p.say())\nend")), "woof\nmeow\n");
    assert!(err(&format!("{src}struct Rock\nend\nlet s: Speak = Rock {{ }}")).starts_with("COMPILE"));
    assert_eq!(out("trait Named\nfun name(self) -> string\nend\nstruct T\nend\nimpl Named for T\npublic fun name(self) -> string\n\"t\"\nend\nend\nfun hi<N: Named>(n: N) -> string\n\"hi \" + n.name()\nend\nprint(hi(T { }))"), "hi t\n");
}

#[test]
fn modules() {
    let math = "export add, Counter, PI\nconst PI = 3\nfun add(a: int, b: int) -> int\na + b\nend\nstruct Counter\npublic n: int = 0\nend\nimpl Counter\npublic fun inc(self)\nself.n += 1\nend\nend\nprint(\"math loaded\")";
    let o = run_files(
        &[
            ("main.q", "import add as plus, Counter from \"math.q\"\nimport \"math.q\" as m\nprint(plus(1, 2))\nprint(m.add(3, 4))\nlet c = Counter { }\nc.inc()\nprint(c.n)\nlet d = m.Counter { n: 5 }\nprint(d.n)\nprint(m.PI)"),
            ("math.q", math),
        ],
        None,
        Limits::default(),
    );
    assert_eq!(o.error, None);
    assert_eq!(o.output, "math loaded\n3\n7\n1\n5\n3\n");
}

#[test]
fn module_errors() {
    let o = run_files(&[("main.q", "import nope from \"x.q\"\n")], None, Limits::default());
    assert!(o.error.unwrap().contains("not found"));
    let o = run_files(&[("main.q", "import a from \"x.q\"\n"), ("x.q", "fun a()\nend")], None, Limits::default());
    assert!(o.error.unwrap().contains("not exported"));
    let o = run_files(&[("main.q", "import a from \"a.q\"\n"), ("a.q", "import b from \"b.q\"\nexport b\nfun a()\nend"), ("b.q", "import a from \"a.q\"\nfun b()\nend\nexport b")], None, Limits::default());
    assert!(o.error.unwrap().contains("circular"));
    let o = run_files(&[("main.q", "import a from \"../x.q\"\n")], None, Limits::default());
    assert!(o.error.unwrap().contains("must be relative"));
    // paths are relative to the importing file
    let o = run_files(
        &[("main.q", "import f from \"lib/a.q\"\nprint(f())"), ("lib/a.q", "import g from \"b.q\"\nfun f() -> int\ng() + 1\nend\nexport f"), ("lib/b.q", "fun g() -> int\n41\nend\nexport g")],
        None,
        Limits::default(),
    );
    assert_eq!(o.error, None);
    assert_eq!(o.output, "42\n");
}

#[test]
fn modules_have_separate_ids() {
    // both files have a function using a parameter at the same position
    let o = run_files(
        &[
            ("main.q", "import add from \"m.q\"\nfun local(p: int, q: int) -> int\np * q\nend\nprint(\"s = {add(1, 2)} {local(3, 4)}\")"),
            ("m.q", "fun add(a: int, b: int) -> int\na + b\nend\nexport add"),
        ],
        None,
        Limits::default(),
    );
    assert_eq!(o.error, None);
    assert_eq!(o.output, "s = 3 12\n");
}

#[test]
fn input_output() {
    let o = run_files(&[("main.q", "print(\"name?\")\nlet n = read()\nprint(\"hi {n}\")")], Some(&["Zoe"]), Limits::default());
    assert_eq!(o.output, "name?\nhi Zoe\n");
    let o = run_files(&[("main.q", "let n = read()\nlet m = read()")], Some(&["only"]), Limits::default());
    assert!(o.error.unwrap().starts_with("R040"));
    let o = run_files(&[("main.q", "let n = read()")], None, Limits::default());
    assert!(o.error.unwrap().contains("not available"));
}

#[test]
fn runtime_failures() {
    assert!(err("panic(\"boom\")").contains("panic: boom"));
    assert!(err("assert(1 > 2, \"math\")").contains("assertion failed: math"));
    assert!(err("assert(false)").contains("assertion failed"));
    assert_eq!(out("assert(1 < 2)\nprint(\"fine\")"), "fine\n");
    let o = run_files(&[("main.q", "while true do\nend")], None, Limits { max_steps: 10_000, ..Limits::default() });
    assert!(o.error.unwrap().starts_with("R900"));
    assert!(err("fun f(n: int) -> int\nf(n + 1)\nend\nprint(f(0))").starts_with("R901"));
    let o = run_files(&[("main.q", "for i in 0..100000 do\nprint(\"xxxxxxxxxx\")\nend")], None, Limits { max_output: 1000, ..Limits::default() });
    assert!(o.error.unwrap().starts_with("R902"));
}

#[test]
fn output_before_error_is_kept() {
    let o = run("print(\"a\")\nprint(1 / 0)\nprint(\"b\")");
    assert_eq!(o.output, "a\n");
    assert!(o.error.unwrap().starts_with("R011"));
}

#[test]
fn deep_recursion_within_limits() {
    assert_eq!(out("fun d(n: int) -> int\nif n == 0 then return 0 end\n1 + d(n - 1)\nend\nprint(d(900))"), "900\n");
}

#[test]
fn comments() {
    assert_eq!(out("-- one\nprint(1) -- two\n--( block\n comment --)\nprint(2)\n--\" doc --\"\nprint(3)\nlet a = 5 --1\nprint(a)"), "1\n2\n3\n5\n");
}

#[test]
fn multiline_expressions() {
    assert_eq!(out("let total = 1 +\n  2 +\n  3\nprint(total)\nprint([\n 1,\n 2,\n].len())\nfun f(a: int,\n b: int) -> int\na + b\nend\nprint(f(\n1,\n2\n))"), "6\n2\n3\n");
}

#[test]
fn static_checks_catch_common_mistakes() {
    for src in [
        "let x = 1\nlet x = 2",
        "print(y)",
        "let x: int = 1.5",
        "fun f(a: int) -> int\na\nend\nprint(f(\"s\"))",
        "let a = [1, \"b\"]",
        "let a = []",
        "struct S\nend\nlet s = S { z: 1 }",
        "if 1 then end",
        "let x = 1\nx.foo()",
        "return 1\nfun f()\nend",
        "let f = fun(x: int) -> int\nx\nend\nf()",
        "let x: int? = none\nlet y: int = x",
    ] {
        let o = run(src);
        let e = o.error.unwrap_or_default();
        assert!(e.starts_with("COMPILE"), "expected a compile error for:\n{src}\ngot: {e:?}");
    }
}

#[test]
fn enum_methods_and_statics() {
    let src = "enum Dir\nUp\nDown\nend\nimpl Dir\npublic fun flip(self) -> Dir\nmatch self\ncase Dir.Up then Dir.Down\ncase Dir.Down then Dir.Up\nend\nend\npublic fun first() -> Dir\nDir.Up\nend\nend\nprint(Dir.first().flip())\nprint(Dir.Down.flip() == Dir.Up)";
    assert_eq!(out(src), "Dir.Down\ntrue\n");
}

#[test]
fn memory_and_nesting_limits() {
    let o = run_files(&[("main.q", "let a: array<int> = []\nwhile true do\na[] = 1\nend")], None, Limits { max_alloc: 1000, ..Limits::default() });
    assert!(o.error.unwrap().starts_with("R903"));
    let o = run_files(&[("main.q", "let s = \"abc\"\nwhile true do\ns = s + s\nend")], None, Limits { max_alloc: 10_000, ..Limits::default() });
    assert!(o.error.unwrap().starts_with("R903"));
    let o = run_files(&[("main.q", "print(\"x\".repeat(1000000))")], None, Limits { max_alloc: 1000, ..Limits::default() });
    assert!(o.error.unwrap().starts_with("R903"));
    // absurd nesting is an error, not a crash
    let deep = format!("let x = {}1{}", "(".repeat(5000), ")".repeat(5000));
    assert!(err(&deep).contains("nested too deeply"));
    let deep_blocks = format!("{}print(1){}", "if true then\n".repeat(3000), "\nend".repeat(3000));
    assert!(err(&deep_blocks).contains("nested too deeply"));
}

#[test]
fn error_locations() {
    // the rendered message points at the right place
    let std = |src: &str| {
        let mut host = MemoryHost::new();
        match compile("main.q", src, &mut host) {
            Ok(_) => String::new(),
            Err(e) => e.render(),
        }
    };
    let r = std("let a = 1\nlet b: string = a");
    assert!(r.contains("main.q:2:17"), "{r}");
    assert!(r.contains("expected `string`, found `int`"), "{r}");
    let r = std("let é = \"héé\" + 1");
    assert!(r.contains("main.q:1"), "{r}");
}

#[test]
fn many_errors_at_once() {
    let mut host = MemoryHost::new();
    let src = "let a: int = \"x\"\nlet b: string = 1\nprint(c)\nfun f() -> int\nend\n";
    let e = match compile("main.q", src, &mut host) {
        Ok(_) => panic!("expected errors"),
        Err(e) => e,
    };
    assert!(e.diagnostics.len() >= 4, "{}", e.render());
}

#[test]
fn narrowing_follows_assignments() {
    // the right side of `cur = cur.next` still sees `cur` as present
    let src = "struct Node\npublic value: int\npublic next: Node?\nend\nlet head = Node { value: 1, next: Node { value: 2, next: none } }\nlet cur: Node? = head\nlet total = 0\nwhile cur != none do\ntotal += cur.value\ncur = cur.next\nend\nprint(total)";
    assert_eq!(out(src), "3\n");
    // after assigning a present value, the variable is usable without a check
    assert_eq!(out("let x: int? = none\nx = 5\nprint(x + 1)"), "6\n");
    // after assigning `none` it is not
    assert!(err("let x: int? = 1\nx = none\nprint(x + 1)").starts_with("COMPILE"));
    // a field cannot be narrowed in place, and the message says what to do
    let e = err("struct N\npublic next: N?\npublic v: int\nend\nlet n = N { next: none, v: 1 }\nif n.next != none then\nprint(n.next.v)\nend");
    assert!(e.contains("copy it into a variable"), "{e}");
}

#[test]
fn beginner_programs() {
    let o = out(include_str!("../../../examples/beginner.q"));
    assert!(o.starts_with("1 2 Fizz 4 Buzz"), "{o}");
    assert!(o.contains("[1, 2, 5, 7, 9]\n6\ngnalq\n"), "{o}");
    assert!(o.ends_with("120\n8\n2\n"), "{o}");
}

#[test]
fn advanced_generics() {
    let o = out(include_str!("../../../examples/advanced.q"));
    assert!(o.starts_with("20\n20\n1\n[\"n1\", \"n2\", \"n3\"]\n[2, 4, 6]\n10\n"), "{o}");
    assert!(o.ends_with("true\nfalse\ntrue\n[\"hell0\", \"w0rld\"]\n"), "{o}");
}
