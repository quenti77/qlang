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

fn program(rng: &mut Rng) -> String {
    let mut src = String::new();
    // a helper called from the others (and recursive in a bounded way)
    src += "fun helper(a: int, b: int) -> int\n  if a > b then a - b else b - a end\nend\n";
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
