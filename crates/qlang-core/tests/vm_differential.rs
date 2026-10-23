//! The typed register machine and the tree walker must agree on every program: same output,
//! same error (code and message), same line for the error. Random programs made only of
//! numeric functions (the kind the machine compiles) are run both ways.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// A random `int` expression over the names in scope, at most `depth` deep.
fn int_expr(rng: &mut Rng, vars: &[&str], depth: usize) -> String {
    if depth == 0 || rng.below(4) == 0 {
        return match rng.below(3) {
            0 if !vars.is_empty() => rng.pick(vars).to_string(),
            1 => format!("{}", rng.below(20) as i64 - 5),
            _ => rng
                .pick(&["0", "1", "2", "7", "9223372036854775807"])
                .to_string(),
        };
    }
    let a = int_expr(rng, vars, depth - 1);
    let b = int_expr(rng, vars, depth - 1);
    match rng.below(9) {
        0 => format!("({a} + {b})"),
        1 => format!("({a} - {b})"),
        2 => format!("({a} * {b})"),
        3 => format!("({a} div {b})"),
        4 => format!("({a} mod {b})"),
        5 => format!("(-{a})"),
        6 => format!(
            "(if {} then {a} else {b} end)",
            bool_expr(rng, vars, depth - 1)
        ),
        7 => format!("helper({a}, {b})"),
        _ => format!("({a} + 1)"),
    }
}

fn bool_expr(rng: &mut Rng, vars: &[&str], depth: usize) -> String {
    let a = int_expr(rng, vars, depth.saturating_sub(1));
    let b = int_expr(rng, vars, depth.saturating_sub(1));
    match rng.below(8) {
        0 => format!("({a} < {b})"),
        1 => format!("({a} <= {b})"),
        2 => format!("({a} > {b})"),
        3 => format!("({a} >= {b})"),
        4 => format!("({a} == {b})"),
        5 => format!("({a} != {b})"),
        6 => format!("(({a} < {b}) and ({b} != 0))"),
        _ => format!("(not ({a} == {b}))"),
    }
}

fn float_expr(rng: &mut Rng, vars: &[&str], depth: usize) -> String {
    if depth == 0 || rng.below(3) == 0 {
        return match rng.below(3) {
            0 if !vars.is_empty() => rng.pick(vars).to_string(),
            1 => format!("{:.2}", rng.below(2000) as f64 / 40.0 - 10.0),
            _ => rng.pick(&["0.0", "1.5", "-2.25", "1e300"]).to_string(),
        };
    }
    let a = float_expr(rng, vars, depth - 1);
    let b = float_expr(rng, vars, depth - 1);
    match rng.below(18) {
        0 => format!("({a} + {b})"),
        1 => format!("({a} - {b})"),
        2 => format!("({a} * {b})"),
        3 => format!("({a} / {b})"),
        4 => format!("({a} mod {b})"),
        5 => format!("({a} ** 2)"),
        6 => format!("{a}.abs()"),
        7 => format!("{a}.sqrt()"),
        8 => format!("{a}.max({b})"),
        9 => format!("{a}.min({b})"),
        10 => format!("{a}.sin()"),
        11 => format!("{a}.ln()"),
        12 => format!("{a}.atan2({b})"),
        13 => format!("{a}.clamp(-1.5, 2.5)"),
        14 => format!("{a}.signum()"),
        15 => format!("({} as float)", int_expr(rng, &[], 1)),
        16 => format!("({a} + {})", rng.below(5)),
        _ => format!("(-{a})"),
    }
}

static UNIQUE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn unique() -> usize {
    UNIQUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Statements that use and change the integer variables `vars` (`acc` among them).
fn stmts(rng: &mut Rng, vars: &[&str], depth: usize, loops: bool) -> String {
    let mut out = String::new();
    for _ in 0..1 + rng.below(3) {
        let target = rng.pick(vars);
        match rng.below(if depth > 0 { 8 } else { 3 }) {
            0 => out += &format!("{target} = {}\n", int_expr(rng, vars, 3)),
            1 => out += &format!("{target} += {}\n", int_expr(rng, vars, 2)),
            2 => out += &format!("{target} -= {}\n", int_expr(rng, vars, 2)),
            3 => {
                out += &format!(
                    "if {} then\n{}else\n{}end\n",
                    bool_expr(rng, vars, 2),
                    stmts(rng, vars, depth - 1, loops),
                    stmts(rng, vars, depth - 1, loops)
                )
            }
            4 if loops => {
                let k = format!("k{}", unique());
                let mut inner: Vec<&str> = vars.to_vec();
                inner.push(&k);
                out += &format!(
                    "for {k} in 0..{} do\n{}if {target} > 1000000 then break end\nend\n",
                    rng.below(12),
                    stmts(rng, &inner, depth - 1, false)
                );
            }
            6 if loops => {
                let k = format!("m{}", unique());
                let mut inner: Vec<&str> = vars.to_vec();
                inner.push(&k);
                let step = rng.pick(&["1", "2", "3", "-1", "-2", "0"]);
                let (lo, hi) = if step.starts_with('-') {
                    (rng.below(15), 0)
                } else {
                    (0, rng.below(15))
                };
                out += &format!(
                    "for {k} in {lo}..{}{hi} step {step} do\n{}end\n",
                    if rng.below(2) == 0 { "=" } else { "" },
                    stmts(rng, &inner, depth - 1, false)
                );
            }
            5 if loops => {
                let w = format!("w{}", unique());
                out += &format!(
                    "let {w} = 0\nwhile {w} < {} do\n{w} += 1\n{}end\n",
                    rng.below(8),
                    stmts(rng, vars, depth - 1, false)
                );
            }
            7 if depth > 1 => out += &format!("{target} += weird({})\n", int_expr(rng, vars, 1)),
            _ => {
                let t = format!("t{}", unique());
                out += &format!("let {t} = {}\n{target} += {t}\n", int_expr(rng, vars, 2));
            }
        }
    }
    out
}

/// An index into the array `xs`: usually valid, sometimes not.
fn index_expr(rng: &mut Rng, vars: &[&str]) -> String {
    match rng.below(6) {
        0 => format!("{}", rng.below(8)),
        1 => format!("{}", -(rng.below(3) as i64)),
        2 => format!("({}) mod (xs.len() + 1)", int_expr(rng, vars, 1)),
        _ => format!("({}) mod 5", int_expr(rng, vars, 1)),
    }
}

/// Statements on the array `xs` (array<int>), with `acc` and `n` to compute with.
fn array_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        match rng.below(if depth > 0 { 19 } else { 15 }) {
            0 => {
                out += &format!(
                    "xs[{}] = {}\n",
                    index_expr(rng, &vars),
                    int_expr(rng, &vars, 2)
                )
            }
            1 => {
                out += &format!(
                    "xs[{}] += {}\n",
                    index_expr(rng, &vars),
                    int_expr(rng, &vars, 1)
                )
            }
            2 => out += &format!("acc += xs[{}]\n", index_expr(rng, &vars)),
            3 => out += &format!("xs[] = {}\n", int_expr(rng, &vars, 2)),
            4 => out += &format!("xs.push({})\n", int_expr(rng, &vars, 1)),
            5 => out += "acc += xs.len()\n",
            6 => out += "acc += xs.sum()\n",
            7 => out += "xs.sort()\n",
            8 => out += "xs.reverse()\n",
            9 => {
                out += &format!(
                    "if xs.contains({}) then acc += 1 end\n",
                    int_expr(rng, &vars, 1)
                )
            }
            10 => {
                out += &format!(
                    "xs.insert({}, {})\n",
                    index_expr(rng, &vars),
                    int_expr(rng, &vars, 1)
                )
            }
            11 => out += &format!("acc += xs.remove({})\n", index_expr(rng, &vars)),
            12 => out += "if xs.is_empty() then n += 1 end\n",
            13 => out += "let ys = xs\nys.push(acc)\n",
            14 => out += &format!("acc = acc + {}\n", int_expr(rng, &vars, 2)),
            15 => {
                let u = unique();
                out += &format!(
                    "for x{u} in xs do\n  acc += x{u}\n{}end\n",
                    if rng.below(2) == 0 {
                        "  if acc > 1000 then break end\n"
                    } else {
                        ""
                    }
                );
            }
            16 => {
                let u = unique();
                out += &format!("for i{u}, x{u} in xs do\n  acc += i{u} * x{u} mod 7\nend\n");
            }
            17 => {
                let u = unique();
                out += &format!(
                    "for j{u} in 0..{} do\n{}end\n",
                    rng.below(6),
                    array_stmts(rng, depth - 1)
                );
            }
            _ => out += "if n > 2 then xs.clear() end\n",
        }
    }
    out
}

fn array_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += &format!(
        "fun work(xs: array<int>, n: int) -> int\n  let acc = 0\n  let n = n\n{}  acc\nend\n",
        array_stmts(rng, 2)
    );
    src += "fun total(xs: array<int>) -> int\n  let t = 0\n  for x in xs do\n    t += x\n  end\n  t\nend\n";
    src += "fun same(xs: array<int>) -> array<int>\n  xs[0] += 1\n  xs\nend\n";
    src += "fun fl(fs: array<float>) -> float\n  let t = 0.0\n  for i in 0..fs.len() do\n    fs[i] = fs[i] * 1.5 + i\n    t += fs[i]\n  end\n  t + fs.sum()\nend\n";
    src += "fun bl(bs: array<bool>) -> int\n  let c = 0\n  for i in 0..bs.len() do\n    bs[i] = not bs[i]\n    if bs[i] then c += 1 end\n  end\n  c\nend\n";
    // functions that make their own arrays: some come back, some are dropped on return
    src += "fun mk(k: int) -> array<int>\n  let a = [k, k + 1]\n  a.push(k * 2)\n  a\nend\n";
    src += "fun mk2(k: int) -> int\n  let t = mk(k)\n  let u: array<float> = [1.5]\n  u.push(2.5)\n  t[2] + t.len() + u.sum().floor()\nend\n";
    src += "fun rec(k: int) -> array<int>\n  if k <= 0 then\n    [0]\n  else\n    let r = rec(k - 1)\n    r.push(k)\n    r\n  end\nend\n";
    src += "print(mk2(3))\nprint(mk(2))\nprint(rec(4))\nprint(total(rec(5)))\n";
    src += "let data = [3, 1, 4, 1, 5, 9, 2, 6]\nlet fdata = [1.5, 2.5, -3.25]\nlet bdata = [true, false, false, true]\n";
    for _ in 0..3 {
        src += &format!("print(work(data, {}))\nprint(data)\n", rng.below(5));
    }
    src += "print(total(data))\nprint(same(data))\nprint(fl(fdata))\nprint(fdata)\nprint(bl(bdata))\nprint(bdata)\n";
    // loops of the top level over the arrays
    src += "let acc = 0\nlet n = 1\n";
    src += &array_stmts_top(rng);
    src += "print(acc)\nprint(data)\n";
    src
}

/// Top-level statements over `data` (the machine compiles the loops among them).
fn array_stmts_top(rng: &mut Rng) -> String {
    let vars = ["acc", "n"];
    let mut out = String::new();
    for _ in 0..3 {
        let u = unique();
        match rng.below(4) {
            0 => {
                out += &format!(
                    "for i{u} in 0..data.len() do\n  data[i{u}] = data[i{u}] * 2 + {}\n  acc += data[i{u}]\nend\n",
                    int_expr(rng, &vars, 1)
                )
            }
            1 => out += &format!("for x{u} in data do\n  acc += x{u} mod 5\nend\n"),
            2 => {
                out += &format!(
                    "let k{u} = 0\nwhile k{u} < {} do\n  data[] = k{u} + acc mod 3\n  k{u} += 1\nend\n",
                    rng.below(6)
                )
            }
            _ => {
                out += &format!(
                    "for i{u} in 0..{} do\n  acc += data[i{u}]\nend\n",
                    rng.below(14)
                )
            }
        }
    }
    out
}

/// A string expression over `s`, `t` (strings) and `acc`, `n` (ints).
fn str_expr(rng: &mut Rng, depth: usize) -> String {
    let lit = || -> &'static str { "ab" };
    let _ = lit;
    let vars = ["acc", "n"];
    let atoms = [
        "s",
        "t",
        "\"x\"",
        "\"Ab \"",
        "\"\"",
        "\"{acc}\"",
        "\"<{n}>\"",
    ];
    if depth == 0 {
        return rng.pick(&atoms).to_string();
    }
    match rng.below(14) {
        0 => format!(
            "{} + {}",
            str_expr(rng, depth - 1),
            str_expr(rng, depth - 1)
        ),
        1 => format!(
            "\"{{{}}}-{{{}}}\"",
            int_expr(rng, &vars, 1),
            rng.pick(&["s", "t"])
        ),
        2 => format!("{}.upper()", str_expr(rng, depth - 1)),
        3 => format!("{}.lower()", str_expr(rng, depth - 1)),
        4 => format!("{}.trim()", str_expr(rng, depth - 1)),
        5 => format!(
            "{}.replace(\"a\", \"{}\")",
            str_expr(rng, depth - 1),
            rng.pick(&["", "AA", "-"])
        ),
        6 => format!("{}.repeat({})", str_expr(rng, depth - 1), rng.below(4)),
        7 => format!(
            "{}.substring({}, {})",
            str_expr(rng, depth - 1),
            rng.below(3),
            rng.below(6)
        ),
        8 => format!("{}.reverse()", str_expr(rng, depth - 1)),
        9 => format!("({}).capitalize()", str_expr(rng, depth - 1)),
        10 => format!(
            "{}.pad_left({}, \"0\")",
            str_expr(rng, depth - 1),
            rng.below(8)
        ),
        11 => format!(
            "\"{{{}}} {{{}}}\"",
            int_expr(rng, &vars, 1),
            float_expr(rng, &["1.5"], 1)
        ),
        _ => rng.pick(&atoms).to_string(),
    }
}

fn str_bool(rng: &mut Rng) -> String {
    let a = str_expr(rng, 1);
    let b = str_expr(rng, 1);
    match rng.below(9) {
        0 => format!("{a} == {b}"),
        1 => format!("{a} != {b}"),
        2 => format!("{a} < {b}"),
        3 => format!("{a} >= {b}"),
        4 => format!("{a}.contains({b})"),
        5 => format!("{a}.starts_with({b})"),
        6 => format!("{a}.ends_with({b})"),
        7 => format!("{a}.is_empty()"),
        _ => format!("{a}.eq_ignore_case({b})"),
    }
}

/// Statements on the strings `s` and `t`, with `acc` and `n` to compute with.
fn str_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        match rng.below(if depth > 0 { 14 } else { 10 }) {
            0 => out += &format!("s = {}\n", str_expr(rng, 2)),
            1 => out += &format!("s += {}\n", str_expr(rng, 1)),
            2 => out += &format!("t = {}\n", str_expr(rng, 2)),
            3 => out += "acc += s.len()\n",
            4 => out += &format!("acc += {}\n", int_expr(rng, &vars, 2)),
            5 => out += &format!("if {} then acc += 1 else n += 1 end\n", str_bool(rng)),
            6 => out += "if s.len() > 40 then s = s.substring(0, 5) end\n",
            7 => out += "print(\"{s}|{t}|{acc}\")\n",
            8 => out += &format!("print({})\n", str_expr(rng, 2)),
            9 => out += "acc += t.count(\"a\") + t.code()\n",
            10 => {
                let u = unique();
                out += &format!(
                    "for i{u} in 0..{} do\n{}if s.len() > 60 then break end\nend\n",
                    rng.below(7),
                    str_stmts(rng, depth - 1)
                );
            }
            11 => {
                let u = unique();
                out += &format!(
                    "let w{u} = 0\nwhile w{u} < {} do\n  w{u} += 1\n{}end\n",
                    rng.below(6),
                    str_stmts(rng, depth - 1)
                );
            }
            12 => {
                out += &format!(
                    "if {} then\n{}else\n{}end\n",
                    str_bool(rng),
                    str_stmts(rng, depth - 1),
                    str_stmts(rng, depth - 1)
                )
            }
            _ => {
                let u = unique();
                out +=
                    &format!("let tmp{u} = [acc, n]\ntmp{u}.push(s.len())\nacc += tmp{u}.sum()\n");
            }
        }
    }
    out
}

fn string_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += &format!(
        "fun work(s: string, t: string, n: int) -> string\n  let acc = 0\n  let n = n\n  let s = s\n  let t = t\n{}  \"{{s}}/{{t}}/{{acc}}/{{n}}\"\nend\n",
        str_stmts(rng, 2)
    );
    src += &format!(
        "fun count(s: string, t: string) -> int\n  let acc = 0\n  let n = 0\n  let s = s\n  let t = t\n{}  acc + n + s.len()\nend\n",
        str_stmts(rng, 2)
    );
    src += "fun join(n: int, sep: string) -> string\n  let out = \"\"\n  for i in 0..n do\n    if i > 0 then out += sep end\n    out += \"{i}\"\n  end\n  out\nend\n";
    src +=
        "fun pick(flag: bool, a: string, b: string) -> string\n  if flag then a else b end\nend\n";
    src += "let gs = \"g\"\nfun bumpg(k: int) -> int\n  gs += \"{k}\"\n  gs.len()\nend\n";
    for _ in 0..3 {
        src += &format!(
            "print(work({}, {}, {}))\n",
            rng.pick(&["\"abc\"", "\"\"", "\"Banana\"", "\" pad \""]),
            rng.pick(&["\"x\"", "\"aaa\"", "\"\""]),
            rng.below(5)
        );
    }
    src += &format!(
        "print(count(\"hello\", \"world\"))\nprint(join({}, \", \"))\n",
        rng.below(6)
    );
    src += "print(pick(true, \"yes\", \"no\"))\nprint(pick(false, \"yes\", \"no\"))\nprint(bumpg(3))\nprint(gs)\n";
    // loops of the top level over strings
    src += "let acc = 0\nlet n = 1\nlet s = \"top\"\nlet t = \"T\"\n";
    src += &str_stmts(rng, 2);
    src += "print(s)\nprint(t)\nprint(acc)\n";
    src
}

/// Statements on the structures `p` (a `P`) and `q` (a `Q`, which extends `P`), with `acc`,
/// `n` (ints) and `s` (a string) to compute with.
fn struct_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let who = |rng: &mut Rng| rng.pick(&["p", "q"]);
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        match rng.below(if depth > 0 { 20 } else { 16 }) {
            0 => out += &format!("{}.a = {}\n", who(rng), int_expr(rng, &vars, 2)),
            1 => out += &format!("{}.a += {}\n", who(rng), int_expr(rng, &vars, 1)),
            2 => out += &format!("{}.b = {}.b * 1.5 - 0.25\n", who(rng), who(rng)),
            3 => out += &format!("{}.s += \"x{{n}}\"\n", who(rng)),
            4 => out += &format!("{0}.ok = not {0}.ok\n", who(rng)),
            5 => out += &format!("acc += {}.bump({})\n", who(rng), int_expr(rng, &vars, 1)),
            6 => out += &format!("acc += {}.combine({})\n", who(rng), who(rng)),
            7 => out += "q.c += 1\n",
            8 => out += "acc += q.c + q.a + p.a\n",
            9 => {
                let u = unique();
                out += &format!(
                    "let r{u} = P {{ a: {}, b: 1.5, s: \"z\" }}\nacc += r{u}.a + r{u}.label().len()\n",
                    int_expr(rng, &vars, 1)
                );
            }
            10 => {
                let u = unique();
                out += &format!("let r{u} = P.make({})\np = r{u}\n", int_expr(rng, &vars, 1));
            }
            11 => out += &format!("print({}.label())\n", who(rng)),
            12 => out += &format!("s = {}.label() + {}.label()\n", who(rng), who(rng)),
            13 => out += &format!("{}.scale(0.5 + n)\n", who(rng)),
            14 => out += &format!("if {}.ok then acc += 1 else n += 1 end\n", who(rng)),
            15 => out += &format!("acc += {}\n", int_expr(rng, &vars, 2)),
            16 => {
                let u = unique();
                out += &format!(
                    "for i{u} in 0..{} do\n{}if acc > 100000 then break end\nend\n",
                    rng.below(7),
                    struct_stmts(rng, depth - 1)
                );
            }
            17 => {
                let u = unique();
                out += &format!(
                    "let w{u} = 0\nwhile w{u} < {} do\n  w{u} += 1\n{}end\n",
                    rng.below(5),
                    struct_stmts(rng, depth - 1)
                );
            }
            18 => {
                out += &format!(
                    "if {}.a > {} then\n{}else\n{}end\n",
                    who(rng),
                    rng.below(10),
                    struct_stmts(rng, depth - 1),
                    struct_stmts(rng, depth - 1)
                )
            }
            19 => out += &format!("acc += {}.odd()\n", who(rng)),
            _ => out += "n += p.a mod 3\n",
        }
    }
    out
}

fn struct_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += "struct P\n  public a: int\n  public b: float\n  public s: string\n  public ok: bool = true\nend\n";
    src += "struct Q extends P\n  public c: int = 5\nend\n";
    src += &format!(
        "impl P\n  public fun bump(self, k: int) -> int\n    self.a += k\n    self.a mod {}\n  end\n  public fun label(self) -> string\n    \"{{self.a}}:{{self.s}}:{{self.b}}\"\n  end\n  public fun scale(self, f: float)\n    self.b = self.b * f\n  end\n  public fun combine(self, o: P) -> int\n    self.a + o.a * 2\n  end\n  public fun make(k: int) -> P\n    P {{ a: k, b: 0.5, s: \"m\" }}\n  end\n  public fun odd(self) -> int\n    let m = {{\"k\": self.a mod 4}}\n    let v = m[\"k\"]\n    if v == none then 0 else v end\n  end\nend\n",
        rng.pick(&["7", "3", "11"])
    );
    src += "impl Q\n  public override fun bump(self, k: int) -> int\n    self.c += 1\n    self.a - k\n  end\n  public override fun label(self) -> string\n    \"Q{self.c}\" + \"{self.a}\"\n  end\nend\n";
    src += &format!(
        "fun work(p: P, q: Q, n: int) -> int\n  let acc = 0\n  let n = n\n  let s = \"\"\n  let p = p\n{}  acc + n + s.len()\nend\n",
        struct_stmts(rng, 2)
    );
    src += "fun sum_all(p: P, q: Q) -> int\n  p.a + q.a + q.c\nend\n";
    src += "let tp = P { a: 4, b: 2.5, s: \"t\" }\nlet tq = Q { a: 1, b: 1.0, s: \"u\" }\n";
    for _ in 0..3 {
        src += &format!("print(work(tp, tq, {}))\n", rng.below(5));
    }
    src += "print(sum_all(tp, tq))\nprint(tp.label())\nprint(tq.label())\n";
    // loops of the top level over the structures
    src += "let acc = 0\nlet n = 1\nlet s = \"top\"\nlet p = tp\nlet q = tq\n";
    src += &struct_stmts(rng, 2);
    src += "print(s)\nprint(acc)\nprint(p.label())\nprint(q.label())\n";
    src
}

/// Statements on `xs` (array<string>), `g` (array<array<int>>), the strings `s`, `t` and the
/// ints `acc`, `n`.
fn obj_array_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let ix = |rng: &mut Rng, name: &str| -> String {
        match rng.below(5) {
            0 => format!("{}", rng.below(6)),
            1 => format!("-{}", 1 + rng.below(2)),
            _ => format!(
                "({}) mod ({name}.len() + 1)",
                int_expr(rng, &["acc", "n"], 1)
            ),
        }
    };
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        match rng.below(if depth > 0 { 22 } else { 18 }) {
            0 => out += &format!("xs[] = {}\n", str_expr(rng, 1)),
            1 => out += &format!("xs.push({})\n", str_expr(rng, 1)),
            2 => out += &format!("xs[{}] = {}\n", ix(rng, "xs"), str_expr(rng, 1)),
            3 => out += &format!("xs[{}] += \"q{{n}}\"\n", ix(rng, "xs")),
            4 => out += &format!("acc += xs[{}].len()\n", ix(rng, "xs")),
            5 => out += "s = xs.join(\"-\")\n",
            6 => out += "for w in xs do\n  acc += w.len()\n  if acc > 500 then break end\nend\n",
            7 => {
                out += "for i, w in xs do\n  t = w + t\n  if t.len() > 30 then t = \"\" end\n  acc += i\nend\n"
            }
            8 => out += "acc += xs.len()\n",
            9 => out += "xs.sort()\n",
            10 => out += "xs.reverse()\n",
            11 => out += &format!("if xs.contains({}) then n += 1 end\n", str_expr(rng, 1)),
            12 => out += &format!("g[] = [{}, n]\n", int_expr(rng, &vars, 1)),
            13 => out += &format!("acc += g[{}][0]\n", ix(rng, "g")),
            14 => out += &format!("g[{}][1] += 1\n", ix(rng, "g")),
            15 => out += "acc += g.len()\n",
            16 => out += "xs = s.words()\n",
            17 => {
                out += &format!(
                    "if {} then xs.clear() end\n",
                    rng.pick(&["n > 3", "acc > 40", "xs.len() > 6"])
                )
            }
            18 => {
                let u = unique();
                out += &format!(
                    "for i{u} in 0..{} do\n{}if xs.len() > 30 then break end\nend\n",
                    rng.below(6),
                    obj_array_stmts(rng, depth - 1)
                );
            }
            19 => {
                let u = unique();
                out += &format!(
                    "let tmp{u}: array<string> = []\nfor i{u} in 0..{} do\n  tmp{u}[] = \"{{i{u}}}\"\n{}end\nxs = tmp{u}\n",
                    rng.below(5),
                    obj_array_stmts(rng, depth - 1)
                );
            }
            _ => out += "for row in g do\n  row[0] += 1\n  acc += row.len()\nend\n",
        }
    }
    out
}

fn obj_array_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += &format!(
        "fun work(xs: array<string>, g: array<array<int>>, s: string, n: int) -> int\n  let acc = 0\n  let n = n\n  let s = s\n  let t = \"\"\n  let xs = xs\n{}  acc + n + xs.len() + s.len() + t.len()\nend\n",
        obj_array_stmts(rng, 2)
    );
    src += "fun make(n: int) -> array<string>\n  let out: array<string> = []\n  for i in 0..n do\n    out[] = \"w{i}\"\n  end\n  out\nend\n";
    src += "fun table(n: int) -> array<array<int>>\n  let rows: array<array<int>> = []\n  for i in 0..n do\n    rows[] = [i, i * 2]\n  end\n  rows\nend\n";
    for _ in 0..3 {
        src += &format!(
            "print(work(make({}), table({}), {}, {}))\n",
            rng.below(6),
            rng.below(5),
            rng.pick(&["\"a b c\"", "\"\"", "\"hello\""]),
            rng.below(5)
        );
    }
    src += "print(make(3))\nprint(table(2))\n";
    src += "let xs = make(2)\nlet g = table(2)\nlet s = \"x y\"\nlet t = \"T\"\nlet acc = 0\nlet n = 1\n";
    src += &obj_array_stmts(rng, 2);
    src += "print(xs)\nprint(g)\nprint(acc)\nprint(s)\n";
    src
}

/// Statements on the optional values `x`, `y` (int?), `w` (string?), `ns` (array<int?>), with
/// `acc`, `n` (ints) to compute with.
fn nullable_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        let v = rng.pick(&["x", "y"]);
        match rng.below(if depth > 0 { 26 } else { 22 }) {
            0 => {
                out += &format!(
                    "{v} = maybe({}, {})\n",
                    bool_expr(rng, &vars, 1),
                    int_expr(rng, &vars, 1)
                )
            }
            1 => out += &format!("{v} = none\n"),
            2 => out += &format!("{v} = {}\n", int_expr(rng, &vars, 2)),
            3 => out += &format!("{v} = parse(\"{{{}}}\")\n", int_expr(rng, &vars, 1)),
            4 => {
                out += &format!(
                    "{v} = parse({})\n",
                    rng.pick(&["\"7\"", "\"x\"", "\"\"", "w2"])
                )
            }
            5 => out += &format!("if {v} != none then acc += {v} * 2 end\n"),
            6 => out += &format!("if {v} == none then n += 1 else acc += {v} end\n"),
            7 => {
                out += &format!(
                    "if {v} != none and {v} > {} then acc += 1 end\n",
                    rng.below(6)
                )
            }
            8 => out += &format!("if {v} == none or {v} < {} then n += 1 end\n", rng.below(6)),
            9 => out += &format!("acc += (if {v} == none then 100 else {v} end)\n"),
            10 => out += &format!("w = pick({}, \"s{{n}}\")\n", bool_expr(rng, &vars, 1)),
            11 => out += "if w != none then acc += w.len() else n += 2 end\n",
            12 => out += &format!("let t{} = {v}\nif t{0} == none then n += 3 end\n", unique()),
            13 => out += &format!("ns[] = {v}\n"),
            14 => out += &format!("ns.push({})\n", rng.pick(&["none", "acc", "n", "x"])),
            15 => {
                out += "for e in ns do\n  if e != none then acc += e end\n  if acc > 400 then break end\nend\n"
            }
            16 => out += "acc += ns.len()\n",
            17 => out += &format!("{v} = find_in(ns, {})\n", int_expr(rng, &vars, 1)),
            18 => out += "if ns.len() > 6 then ns.clear() end\n",
            19 => out += &format!("acc += {}\n", int_expr(rng, &vars, 2)),
            20 => out += &format!("w = {}.to_string_opt()\n", v),
            21 => out += &format!("{v} = ns.first()\n"),
            22 | 23 => {
                let u = unique();
                out += &format!(
                    "for i{u} in 0..{} do\n{}if ns.len() > 40 then break end\nend\n",
                    rng.below(6),
                    nullable_stmts(rng, depth - 1)
                );
            }
            24 => {
                let u = unique();
                out += &format!(
                    "let w{u} = 0\nwhile w{u} < {} do\n  w{u} += 1\n{}end\n",
                    rng.below(5),
                    nullable_stmts(rng, depth - 1)
                );
            }
            _ => {
                out += &format!(
                    "if {v} != none then\n{}else\n{}end\n",
                    nullable_stmts(rng, depth - 1),
                    nullable_stmts(rng, depth - 1)
                )
            }
        }
    }
    out
}

fn nullable_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += "fun parse(s: string) -> int?\n  s.to_int()\nend\n";
    src += "fun maybe(flag: bool, v: int) -> int?\n  if flag then v else none end\nend\n";
    src += "fun pick(flag: bool, s: string) -> string?\n  if flag then s else none end\nend\n";
    src += "fun find_in(xs: array<int?>, k: int) -> int?\n  for e in xs do\n    if e != none and e > k then\n      return e\n    end\n  end\n  none\nend\n";
    src += &format!(
        "fun work(n: int, w2: string) -> int\n  let acc = 0\n  let n = n\n  let x: int? = none\n  let y: int? = {}\n  let w: string? = none\n  let ns: array<int?> = []\n{}  let r = 0\n  if x != none then r += x end\n  if y != none then r += y end\n  acc + n + r + ns.len()\nend\n",
        rng.pick(&["none", "4", "-2"]),
        nullable_stmts(rng, 2)
    );
    for _ in 0..3 {
        src += &format!(
            "print(work({}, {}))\n",
            rng.below(6),
            rng.pick(&["\"5\"", "\"q\"", "\"\""])
        );
    }
    src += "print(parse(\"12\"))\nprint(parse(\"zz\"))\nprint(maybe(true, 3))\nprint(pick(false, \"s\"))\n";
    src += "let acc = 0\nlet n = 1\nlet x: int? = 3\nlet y: int? = none\nlet w: string? = \"top\"\nlet w2 = \"9\"\nlet ns: array<int?> = [1, none]\n";
    src += &nullable_stmts(rng, 2);
    src += "print(acc)\nprint(n)\nprint(x)\nprint(y)\nprint(w)\nprint(ns)\n";
    src
}

/// A key of the string map `m`.
fn map_key(rng: &mut Rng) -> String {
    match rng.below(4) {
        0 => format!("\"k{}\"", rng.below(4)),
        1 => "\"k{acc mod 4}\"".to_string(),
        2 => "\"n{n mod 3}\"".to_string(),
        _ => "s".to_string(),
    }
}

/// Statements on `m` (map<string, int>), `ms` (map<int, string>), `s` (a string), with `acc`,
/// `n` (ints) to compute with.
fn map_stmts(rng: &mut Rng, depth: usize) -> String {
    let vars = ["acc", "n"];
    let mut out = String::new();
    for _ in 0..1 + rng.below(4) {
        match rng.below(if depth > 0 { 24 } else { 20 }) {
            0 => out += &format!("m[{}] = {}\n", map_key(rng), int_expr(rng, &vars, 2)),
            1 => out += &format!("m[{}] += {}\n", map_key(rng), int_expr(rng, &vars, 1)),
            2 => out += &format!("acc += m.get({}, 0)\n", map_key(rng)),
            3 => out += "acc += m.len()\n",
            4 => out += &format!("if m.has({}) then n += 1 end\n", map_key(rng)),
            5 => {
                let u = unique();
                out += &format!(
                    "let v{u} = m[{}]\nif v{u} != none then acc += v{u} else n += 2 end\n",
                    map_key(rng)
                );
            }
            6 => out += "for k in m.keys() do\n  acc += k.len()\nend\n",
            7 => out += "for v in m.values() do\n  acc += v\n  if acc > 600 then break end\nend\n",
            8 => {
                let u = unique();
                out += &format!(
                    "let r{u} = m.remove({})\nif r{u} != none then acc += r{u} end\n",
                    map_key(rng)
                );
            }
            9 => out += &format!("ms[{}] = \"s{{acc}}\"\n", int_expr(rng, &["acc", "n"], 1)),
            10 => out += &format!("s = ms.get({}, \"-\")\n", int_expr(rng, &["acc", "n"], 1)),
            11 => out += "if m.len() > 6 then m.clear() end\n",
            12 => {
                out +=
                    "for k, v in m do\n  acc += k.len() + v\n  if acc > 600 then break end\nend\n"
            }
            13 => {
                out += "for k, v in ms do\n  s = v + s\n  if s.len() > 30 then s = \"\" end\n  acc += k\nend\n"
            }
            14 => {
                let u = unique();
                out += &format!(
                    "let lit{u}: map<string, int> = {{ \"a\": acc, \"b\": n }}\nacc += lit{u}.len() + lit{u}.get(\"a\", 0)\n"
                );
            }
            15 => out += &format!("acc += {}\n", int_expr(rng, &vars, 2)),
            16 => out += "if ms.has(acc mod 3) then n += 1 end\n",
            17 => out += &format!("let w{} = m.keys()\nacc += w{0}.len()\n", unique()),
            18 => out += "if ms.len() > 5 then ms.clear() end\n",
            19 => out += "n += m.get(\"k0\", 1) mod 3\n",
            20 | 21 => {
                let u = unique();
                out += &format!(
                    "for i{u} in 0..{} do\n{}if m.len() > 30 then break end\nend\n",
                    rng.below(6),
                    map_stmts(rng, depth - 1)
                );
            }
            22 => {
                let u = unique();
                out += &format!(
                    "let w{u} = 0\nwhile w{u} < {} do\n  w{u} += 1\n{}end\n",
                    rng.below(5),
                    map_stmts(rng, depth - 1)
                );
            }
            _ => {
                out += &format!(
                    "if m.has({}) then\n{}else\n{}end\n",
                    map_key(rng),
                    map_stmts(rng, depth - 1),
                    map_stmts(rng, depth - 1)
                )
            }
        }
    }
    out
}

fn map_program(rng: &mut Rng) -> String {
    let mut src = String::new();
    src += &format!(
        "fun work(m: map<string, int>, ms: map<int, string>, s: string, n: int) -> int\n  let acc = 0\n  let n = n\n  let s = s\n{}  acc + n + m.len() + ms.len() + s.len()\nend\n",
        map_stmts(rng, 2)
    );
    src += "fun counts(words: array<string>) -> map<string, int>\n  let c: map<string, int> = {}\n  for w in words do\n    c[w] = c.get(w, 0) + 1\n  end\n  c\nend\n";
    src += "fun table(n: int) -> map<int, string>\n  let t: map<int, string> = {}\n  for i in 0..n do\n    t[i] = \"v{i}\"\n  end\n  t\nend\n";
    for _ in 0..3 {
        src += &format!(
            "print(work(counts([\"a\", \"b\", \"a\", \"k0\"]), table({}), {}, {}))\n",
            rng.below(5),
            rng.pick(&["\"s\"", "\"\"", "\"k1\""]),
            rng.below(5)
        );
    }
    src += "print(counts([\"x\", \"y\", \"x\"]))\nprint(table(3))\n";
    src += "let m = counts([\"p\", \"q\"])\nlet ms = table(2)\nlet s = \"top\"\nlet acc = 0\nlet n = 1\n";
    src += &map_stmts(rng, 2);
    src += "print(m)\nprint(ms)\nprint(acc)\nprint(s)\n";
    src
}

fn program(rng: &mut Rng) -> String {
    let mut src = String::new();
    // a helper called from the others (and recursive in a bounded way)
    src += "fun helper(a: int, b: int) -> int\n  if a > b then a - b else b - a end\nend\n";
    // a function the machine cannot compile (a map): it is called through the interpreter
    src += "fun weird(x: int) -> int\n  let m = {\"k\": x mod 5}\n  let v = m[\"k\"]\n  if v == none then 0 else v + 1 end\nend\n";
    src += &format!(
        "fun rec(n: int, acc: int) -> int\n  if n <= 0 then return acc end\n  rec(n - 1, acc + {})\nend\n",
        int_expr(rng, &["n", "acc"], 2)
    );
    src += &format!(
        "fun f(x: int, y: int) -> int\n  let acc = 0\n{}  acc + {}\nend\n",
        stmts(rng, &["acc", "x", "y"], 2, true),
        int_expr(rng, &["acc", "x", "y"], 3)
    );
    src += &format!(
        "fun g(a: float, b: float) -> float\n  let c = {}\n  c * {}\nend\n",
        float_expr(rng, &["a", "b"], 3),
        float_expr(rng, &["a", "b", "c"], 2)
    );
    // module variables, used by functions and by loops of the top level
    src += "let g1 = 3\nlet g2 = 1.5\nlet flag = true\n";
    src += &format!(
        "fun bump(x: int) -> int\n  g1 += x mod 4\n  g1 + {}\nend\n",
        int_expr(rng, &["x", "g1"], 2)
    );
    src += &format!(
        "fun scale(x: float) -> float\n  g2 = g2 * 0.5 + x\n  g2\nend\nfun readg(x: int) -> int\n  let t = {}\n  t + g1\nend\n",
        int_expr(rng, &["x", "g1"], 2)
    );
    // a function the machine does not compile (it prints), with a numeric loop inside
    src += &format!(
        "fun noisy(n: int) -> int\n  let acc = 0\n  let w = n\n{}  print(\"noisy {{acc}}\")\n  acc\nend\n",
        stmts(rng, &["acc", "w", "n"], 2, true)
    );
    src += "fun h(p: bool, q: int) -> bool\n  p and (q > 3) or (q == 0)\nend\n";
    for _ in 0..4 {
        let (x, y) = (rng.below(30) as i64 - 8, rng.below(30) as i64 - 8);
        src += &format!("print(f({x}, {y}))\n");
    }
    src += &format!("print(rec({}, 1))\n", rng.below(60));
    // loops of the top level over module variables
    for k in 0..3 {
        let acc = format!("acc{k}");
        let vars = ["g1", acc.as_str()];
        src += &format!("let {acc} = 1\n");
        src += &stmts(rng, &vars, 2, true);
        src += &format!("print(g1)\nprint({acc})\n");
    }
    src += &format!(
        "for q in 0..{} do\n  g2 += q * 0.25\n  if q mod 3 == 0 then flag = not flag end\nend\nprint(g2)\nprint(flag)\n",
        rng.below(20)
    );
    src += &format!(
        "print(bump({}))\nprint(scale({:.1}))\nprint(readg({}))\n",
        rng.below(9),
        rng.below(50) as f64 / 10.0,
        rng.below(9)
    );
    src += &format!("print(noisy({}))\n", rng.below(9));
    // a loop that calls a function which changes the variables it uses
    src += &format!(
        "let tot = 0\nfor q in 0..{} do\n  tot += bump(q)\n  g1 += 1\nend\nprint(tot)\nprint(g1)\n",
        rng.below(12)
    );
    src += &format!(
        "print(g({:.2}, {:.2}))\n",
        rng.below(400) as f64 / 10.0 - 20.0,
        rng.below(400) as f64 / 10.0 - 20.0
    );
    src += &format!("print(h({}, {}))\n", rng.below(2) == 0, rng.below(8));
    src
}

static FUNCS: [std::sync::atomic::AtomicU64; 2] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

static STEPS: [std::sync::atomic::AtomicU64; 2] = [
    std::sync::atomic::AtomicU64::new(0),
    std::sync::atomic::AtomicU64::new(0),
];

fn run_both(src: &str) -> (String, String) {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let go = |vm: bool| -> String {
                let mut host = MemoryHost::default();
                let prog = match compile("main.q", &src, &mut host) {
                    Ok(p) => p,
                    Err(e) => return format!("COMPILE {}", e.render()),
                };
                let limits = Limits {
                    max_steps: 3_000_000,
                    ..Limits::default()
                };
                let r = if vm {
                    prog.run(&mut host, limits)
                } else {
                    prog.run_tree_walker(&mut host, limits)
                };
                if vm {
                    FUNCS[0].fetch_add(
                        r.vm_functions.0 as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    FUNCS[1].fetch_add(
                        r.vm_functions.1 as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
                STEPS[usize::from(vm)].fetch_add(r.steps, std::sync::atomic::Ordering::Relaxed);
                match r.error {
                    // the step counts differ on purpose: only the kind of error matters then
                    Some(d) if d.code == "R900" => format!("{}R900", host.output),
                    Some(d) => format!("{}{}", host.output, d.render(&prog.sources)),
                    None => host.output,
                }
            };
            (go(true), go(false))
        })
        .unwrap()
        .join()
        .unwrap()
}

fn sweep(seed: u64, count: usize) {
    sweep_with(seed, count, program);
}

fn sweep_with(seed: u64, count: usize, generate: fn(&mut Rng) -> String) {
    let mut rng = Rng(seed);
    let mut compiled = 0;
    for i in 0..count {
        let src = generate(&mut rng);
        let (vm, walker) = run_both(&src);
        if vm.starts_with("COMPILE") {
            if std::env::var("SHOW_COMPILE_ERRORS").is_ok() && i < 3 {
                eprintln!("{vm}\n{src}");
            }
            continue;
        }
        compiled += 1;
        assert_eq!(vm, walker, "program #{i}:\n{src}");
    }
    assert!(
        compiled > count / 2,
        "too few programs compiled: {compiled}/{count}"
    );
    // the machine really ran them: it counts fewer steps than the tree walker
    let (walker, machine) = (
        STEPS[0].load(std::sync::atomic::Ordering::Relaxed),
        STEPS[1].load(std::sync::atomic::Ordering::Relaxed),
    );
    let (yes, no) = (
        FUNCS[0].load(std::sync::atomic::Ordering::Relaxed),
        FUNCS[1].load(std::sync::atomic::Ordering::Relaxed),
    );
    eprintln!(
        "programs: {compiled}/{count}; steps machine {machine}, tree walker {walker}; functions compiled {yes}, refused {no}"
    );
    assert!(
        machine < walker,
        "the machine was not used ({machine} vs {walker} steps)"
    );
}

#[test]
fn the_machine_and_the_tree_walker_agree() {
    sweep(0x1234_5678_9ABC_DEF1, 300);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_many_more() {
    for seed in 1..=20u64 {
        sweep(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1, 500);
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_arrays() {
    sweep_with(0x0DDB_1A5E_5BAD_5EED, 300, array_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_arrays_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0xD6E8_FEB8_6659_FD93) | 1,
            500,
            array_program,
        );
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_strings() {
    sweep_with(0x57A1_C0DE_FEED_0001, 300, string_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_strings_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0xA24B_AED4_963E_E407) | 1,
            500,
            string_program,
        );
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_structures() {
    sweep_with(0x5712_AC75_0000_0003, 300, struct_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_structures_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0xC13F_A9A9_02A6_328F) | 1,
            500,
            struct_program,
        );
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_arrays_of_objects() {
    sweep_with(0x0B1E_C75A_77A9_0004, 300, obj_array_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_arrays_of_objects_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0x8CB9_2BA7_2F3D_8DD7) | 1,
            500,
            obj_array_program,
        );
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_optional_values() {
    sweep_with(0x0A7B_10A1_0000_0005, 300, nullable_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_optional_values_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0xE703_7ED1_A0B4_28DB) | 1,
            500,
            nullable_program,
        );
    }
}

#[test]
fn the_machine_and_the_tree_walker_agree_on_maps() {
    sweep_with(0x3A95_D1C7_0000_0006, 300, map_program);
}

#[test]
#[ignore]
fn the_machine_and_the_tree_walker_agree_on_maps_in_many_more_programs() {
    for seed in 1..=20u64 {
        sweep_with(
            seed.wrapping_mul(0x94D0_49BB_1331_11EB) | 1,
            500,
            map_program,
        );
    }
}
