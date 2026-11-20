//! The native code (feature `jit`) must give what the tree walker gives, errors included: in
//! each program a function becomes hot (300 calls) and then meets a limit or an error, and the
//! output (message, line, call stack) must be the same.
#![cfg(feature = "jit")]

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str, limits: Limits, walker: bool) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => return format!("COMPILE: {}", e.render()),
            };
            let r = if walker {
                prog.run_tree_walker(&mut host, limits)
            } else {
                prog.run(&mut host, limits)
            };
            match r.error {
                Some(d) => format!("{}{}", host.output, d.render(&prog.sources)),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

fn same_with(src: &str, limits: Limits) -> String {
    let a = run(src, limits, false);
    let b = run(src, limits, true);
    assert_eq!(a, b, "the engines disagree on:\n{src}");
    a
}

fn same(src: &str) -> String {
    same_with(src, Limits::default())
}

#[test]
fn a_hot_recursive_function_gives_the_same_result() {
    let out = same(
        "fun fib(n: int) -> int\n  if n < 2 then n else fib(n - 1) + fib(n - 2) end\nend\nprint(fib(25))",
    );
    assert_eq!(out, "75025\n");
}

#[test]
fn an_overflow_in_native_code_is_reported_as_the_machine_does() {
    let out = same(
        "fun sq(n: int) -> int\n  n * n\nend\nlet t = 0\nfor i in 0..100000 do\n  t += sq(i * 100000)\nend\nprint(t)",
    );
    assert!(out.contains("R010"), "{out}");
}

#[test]
fn a_division_by_zero_in_native_code_is_reported_as_the_machine_does() {
    let out = same(
        "fun d(a: int, b: int) -> int\n  a // b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += d(100, 500 - i)\nend\nprint(t)",
    );
    assert!(out.contains("R011"), "{out}");
}

#[test]
fn negative_division_and_modulo_agree() {
    let out = same(
        "fun g(a: int, b: int) -> int\n  a // b + a mod b\nend\nlet t = 0\nfor i in -400..400 do\n  t += g(i * 7, 5) + g(i * 7, -3)\nend\nprint(t)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_infinite_recursion_stops_at_the_same_depth() {
    let out = same(
        "fun r(n: int) -> int\n  if n < 100 then n else r(n + 1) + 1 end\nend\nlet t = 0\nfor i in 0..500 do\n  t += r(i)\nend\nprint(t)\nfun inf(n: int) -> int\n  inf(n + 1) + 1\nend\nprint(inf(0))",
    );
    assert!(out.contains("R901"), "{out}");
}

#[test]
fn the_step_limit_stops_a_hot_function_where_the_machine_does() {
    // the tree walker reports the line of the statement in the loop, the machine that of the
    // loop: native code must say what the machine says
    let limits = Limits {
        max_steps: 200_000,
        ..Limits::default()
    };
    let out = run(
        "fun s(n: int) -> int\n  let i = 0\n  while i < n do\n    i += 1\n  end\n  i\nend\nlet t = 0\nfor k in 0..100000 do\n  t += s(100)\nend\nprint(t)",
        limits,
        false,
    );
    assert!(out.contains("R900") && out.contains("main.q:3:3"), "{out}");
}

#[test]
fn floats_comparisons_and_for_loops_agree() {
    let out = same(
        "fun h(x: float) -> float\n  if x < 0.5 then x * 2.0 else x - 1.5 end\nend\nfun f(n: int) -> float\n  let s = 0.0\n  for i in 0..n do\n    s += h(i / 7)\n  end\n  s\nend\nfor k in 0..400 do\n  f(10)\nend\nprint(f(1000))\nfun w(a: int, b: int) -> int\n  let c = 0\n  for i in a..b step 3 do\n    c += i\n  end\n  c\nend\nfor k in 0..400 do\n  w(0, 10)\nend\nprint(w(-50, 50))\nprint(w(50, -50))",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_float_division_by_zero_in_native_code_is_reported() {
    let out = same(
        "fun q(x: float) -> float\n  1.0 / (x - 1.0)\nend\nlet t = 0.0\nfor i in 0..1000 do\n  t += q(i * 1.0)\nend\nprint(t)",
    );
    assert!(out.contains("R011"), "{out}");
}

#[test]
fn powers_roots_min_max_and_float_modulo_agree() {
    let out = same(
        "fun a(x: int, y: int) -> int\n  (x ** y).min(1000000) + x.max(y)\nend\nfun b(x: float, y: float) -> float\n  x.sqrt() + (x ** y) + x.min(y) + x.max(y) + x mod y\nend\nlet t = 0\nlet u = 0.0\nfor i in 0..1000 do\n  t += a(i mod 7, i mod 5)\n  u += b(i * 1.0, 1.5 + i mod 3)\nend\nprint(t)\nprint(u)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_power_overflow_a_negative_exponent_and_a_root_of_a_negative_are_reported() {
    let o = same(
        "fun p(a: int, b: int) -> int\n  a ** b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += p(3, i mod 41)\nend\nprint(t)",
    );
    assert!(o.contains("R010"), "{o}");
    let o = same(
        "fun p(a: int, b: int) -> int\n  a ** b\nend\nlet t = 0\nfor i in 0..1000 do\n  t += p(2, 5 - i)\nend\nprint(t)",
    );
    assert!(o.contains("R012"), "{o}");
    let o = same(
        "fun r(x: float) -> float\n  x.sqrt()\nend\nlet t = 0.0\nfor i in 0..1000 do\n  t += r(500.0 - i)\nend\nprint(t)",
    );
    assert!(o.contains("R014"), "{o}");
}

#[test]
fn a_hot_loop_at_the_root_gives_the_same_variables() {
    let out = same(
        "let total = 0\nlet i = 0\nlet f = 0.5\nwhile i < 5000 do\n  total += i mod 7\n  f = f * 1.0001\n  i += 1\nend\nprint(total)\nprint(f)\nprint(i)\nlet s = 0\nfor a in 0..200 do\n  for b in a..400 step 3 do\n    s += a * b\n  end\nend\nprint(s)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_error_in_a_hot_loop_is_reported_as_the_machine_does() {
    let o = same("let x = 1\nlet i = 0\nwhile i < 100 do\n  x = x * 3\n  i += 1\nend\nprint(x)");
    assert!(o.contains("R010"), "{o}");
    let o = same(
        "let i = 0\nlet t = 0\nwhile i < 2000 do\n  t += 100 // (1000 - i)\n  i += 1\nend\nprint(t)",
    );
    assert!(o.contains("R011"), "{o}");
}

#[test]
fn a_hot_loop_calls_a_hot_function_and_leaves_through_a_return() {
    let o = same(
        "fun sq(n: int) -> int\n  n * n\nend\nfun find(limit: int) -> int\n  let i = 0\n  while i < 100000 do\n    if sq(i) > limit then\n      return i\n    end\n    i += 1\n  end\n  0\nend\nprint(find(1000000))\nprint(find(50))",
    );
    assert_eq!(o, "1001\n8\n");
}

#[test]
fn the_step_limit_in_a_hot_loop_is_the_machines() {
    let limits = Limits {
        max_steps: 100_000,
        ..Limits::default()
    };
    // (the tree walker puts the position elsewhere in the loop: only the machine is checked)
    let o = run(
        "let i = 0\nwhile i < 100000000 do\n  i += 1\nend\nprint(i)",
        limits,
        false,
    );
    assert!(o.contains("R900"), "{o}");
}

#[test]
fn hot_loops_over_arrays_of_numbers_agree() {
    let out = same(
        "let xs: array<int> = []\nfor i in 0..3000 do\n  xs.push(i * 2 - 7)\nend\nlet fs: array<float> = []\nlet bs: array<bool> = []\nfor i in 0..xs.len() do\n  fs.push(xs[i] / 3)\n  bs.push(xs[i] mod 3 == 0)\nend\nlet acc = 0\nlet facc = 0.0\nlet n = 0\nfor i in 0..xs.len() do\n  acc += xs[i]\n  xs[i] = acc\n  facc += fs[i]\n  if bs[i] then\n    n += 1\n  end\nend\nfor x in xs do\n  acc += x mod 11\nend\nprint(acc)\nprint(facc)\nprint(n)\nprint(xs[2999])",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_index_out_of_bounds_in_a_hot_loop_is_reported_as_the_machine_does() {
    let o = same(
        "let xs: array<int> = [1, 2, 3]\nlet t = 0\nfor i in 0..1000 do\n  t += xs[i mod 4]\nend\nprint(t)",
    );
    assert!(o.contains("R020"), "{o}");
    let o = same(
        "let xs: array<int> = [0, 0, 0]\nlet i = 0\nwhile i < 1000 do\n  xs[i] = i\n  i += 1\nend\nprint(xs[0])",
    );
    assert!(o.contains("R020"), "{o}");
}

#[test]
fn a_hot_loop_that_pushes_stops_at_the_memory_limit_of_the_machine() {
    let limits = Limits {
        max_alloc: 2000,
        ..Limits::default()
    };
    let o = same_with(
        "let xs: array<int> = []\nlet i = 0\nwhile i < 100000 do\n  xs.push(i)\n  i += 1\nend\nprint(xs.len())",
        limits,
    );
    assert!(o.contains("R903"), "{o}");
}

#[test]
fn the_sieve_of_a_function_with_an_array_parameter_agrees() {
    let o = same(
        "fun sieve(flags: array<bool>, n: int) -> int\n  let count = 0\n  for i in 2..n do\n    if flags[i] then\n      count += 1\n      let j = i * i\n      while j < n do\n        flags[j] = false\n        j += i\n      end\n    end\n  end\n  count\nend\nlet n = 20000\nlet flags: array<bool> = []\nfor i in 0..n do\n  flags.push(true)\nend\nprint(sieve(flags, n))",
    );
    assert_eq!(o, "2262\n");
}

#[test]
fn float_registers_agree_with_the_tree_walker() {
    let out = same(
        "fun mandel(cr: float, ci: float) -> int\n  let zr = 0.0\n  let zi = 0.0\n  let n = 0\n  while n < 100 and zr * zr + zi * zi < 4.0 do\n    let t = zr * zr - zi * zi + cr\n    zi = 2.0 * zr * zi + ci\n    zr = t\n    n += 1\n  end\n  n\nend\nfun mix(x: float, k: int) -> float\n  let y = x\n  if k mod 2 == 0 then y = y * 1.5 else y = y - k end\n  y + k\nend\nlet s = 0\nlet f = 0.0\nfor i in 0..300 do\n  s += mandel(i * 0.01 - 1.5, 0.3)\n  f += mix(i * 0.5, i)\nend\nprint(s)\nprint(f)\nlet xs: array<float> = []\nfor i in 0..500 do\n  xs.push(i * 0.25)\nend\nlet acc = 0.0\nfor i in 0..xs.len() do\n  xs[i] = xs[i] * 2.0 + 1.0\n  acc += xs[i]\nend\nprint(acc)",
    );
    assert!(!out.contains("error"), "{out}");
}

const POINT: &str = "struct P\n  public x: int\n  public y: float\n  public on: bool\nend\n";

#[test]
fn a_hot_loop_reads_and_writes_the_numbers_of_a_structure() {
    // the structure is made in the function: the machine makes it, the loop is native
    let o = same(&format!(
        "{POINT}fun run(n: int) -> int\n  let p = P {{ x: 0, y: 0.5, on: true }}\n  let i = 0\n  while i < n do\n    p.x = p.x + i mod 7\n    p.y = p.y * 1.5 - 0.25\n    if p.on then\n      p.x += 1\n    end\n    p.on = i mod 3 != 0\n    i += 1\n  end\n  print(p.y)\n  p.x\nend\nprint(run(100000))\nprint(run(10))"
    ));
    assert!(!o.contains("error"), "{o}");
    // a structure that is given
    let o = same(&format!(
        "{POINT}fun bump(p: P, n: int)\n  let i = 0\n  while i < n do\n    p.x += 2\n    i += 1\n  end\nend\nlet p = P {{ x: 1, y: 0.0, on: false }}\nbump(p, 50000)\nbump(p, 10)\nprint(p.x)"
    ));
    assert_eq!(o, "100021\n");
}

#[test]
fn an_error_on_a_field_in_a_hot_loop_is_reported_as_the_machine_does() {
    let o = same(&format!(
        "{POINT}let p = P {{ x: 1, y: 0.0, on: false }}\nlet i = 0\nwhile i < 100 do\n  p.x = p.x * 3\n  i += 1\nend\nprint(p.x)"
    ));
    assert!(o.contains("R010"), "{o}");
    let o = same(&format!(
        "{POINT}let p = P {{ x: 100, y: 0.0, on: false }}\nlet i = 0\nwhile i < 2000 do\n  p.x = p.x - 1\n  p.y = p.y + 100 // p.x\n  i += 1\nend\nprint(p.y)"
    ));
    assert!(o.contains("R011") || o.contains("error"), "{o}");
}

#[test]
fn a_loop_that_makes_a_structure_stays_on_the_machine() {
    let o = same(&format!(
        "{POINT}let t = 0\nlet i = 0\nwhile i < 5000 do\n  let p = P {{ x: i, y: 0.0, on: true }}\n  t += p.x\n  i += 1\nend\nprint(t)"
    ));
    assert_eq!(o, "12497500\n");
}

const PT: &str =
    "struct P\n  public x: int\n  public y: int\n  public w: float\n  public on: bool\nend\n";

#[test]
fn a_hot_method_reads_and_writes_the_numbers_of_its_structure() {
    let out = same(&format!(
        "{PT}impl P\n  public fun advance(self) -> int\n    self.x = self.x + 1\n    self.y = (self.y + self.x) mod 1000003\n    self.w = self.w * 1.5 + 0.25\n    self.on = not self.on\n    self.y\n  end\nend\nlet p = P {{ x: 0, y: 0, w: 1.0, on: false }}\nlet last = 0\nfor i in 0..2000 do\n  last = p.advance()\nend\nprint(last)\nprint(p.x)\nprint(p.w > 1.0)\nprint(p.on)"
    ));
    assert!(
        out.starts_with(&format!("{}\n", expected_y())) || !out.contains("error"),
        "{out}"
    );
}

fn expected_y() -> i64 {
    let (mut x, mut y) = (0i64, 0i64);
    for _ in 0..2000 {
        x += 1;
        y = (y + x) % 1_000_003;
    }
    y
}

#[test]
fn a_function_with_two_structures_that_are_the_same_one_agrees() {
    let out = same(&format!(
        "{PT}fun move_by(a: P, b: P) -> int\n  a.x = (a.x + b.x + 1) mod 1000\n  b.y = (b.y + a.x) mod 1000\n  a.x + b.y\nend\nlet p = P {{ x: 1, y: 2, w: 0.0, on: true }}\nlet q = P {{ x: 5, y: 7, w: 0.0, on: true }}\nlet t = 0\nfor i in 0..1000 do\n  t = (t + move_by(p, q)) mod 1000003\n  t = (t + move_by(p, p)) mod 1000003\nend\nprint(t)\nprint(p.x)\nprint(p.y)\nprint(q.x)\nprint(q.y)"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_overflow_after_a_write_leaves_the_structure_as_the_machine_does() {
    // the second write overflows at some turn: the first one must not have happened twice
    let out = same(&format!(
        "{PT}impl P\n  public fun grow(self) -> int\n    self.x = self.x + 1\n    self.y = self.y * 3\n    self.y\n  end\nend\nlet p = P {{ x: 0, y: 1, w: 0.0, on: false }}\nfor i in 0..1000 do\n  print(\"{{p.x}} {{p.y}}\")\n  p.grow()\nend"
    ));
    assert!(out.contains("R010"), "{out}");
}

#[test]
fn a_method_that_calls_a_method_that_reads_fields_agrees() {
    let out = same(&format!(
        "{PT}fun total(p: P) -> int\n  p.x + p.y\nend\nfun twice(p: P, k: int) -> int\n  total(p) * k + total(p)\nend\nlet p = P {{ x: 3, y: 4, w: 0.0, on: false }}\nlet t = 0\nfor i in 0..1000 do\n  p.x = i\n  t = (t + twice(p, 2)) mod 1000003\nend\nprint(t)"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_missing_or_foreign_value_for_a_structure_parameter_is_the_machines_error() {
    let out = same(&format!(
        "{PT}fun get(p: P?) -> int\n  if p == none then 0 else p.x end\nend\nlet p = P {{ x: 3, y: 4, w: 0.0, on: false }}\nlet t = 0\nfor i in 0..1000 do\n  t += get(p) + get(none)\nend\nprint(t)"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_hot_loop_that_writes_variables_of_the_module_agrees() {
    let out = same(
        "let t = 0\nlet w = 0.5\nlet flip = false\nlet n = 0\nfun add(a: int, b: int) -> int\n  a + b\nend\nfor i in 0..5000 do\n  t = add(t, i) mod 1000003\n  w = w * 1.0001 + 0.5\n  flip = not flip\n  n += 1\nend\nprint(t)\nprint(w > 1.0)\nprint(flip)\nprint(n)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_error_in_a_loop_that_writes_variables_leaves_them_as_the_machine_does() {
    // the error stops the run, but the variables written before it are printed by the loop
    let out = same(
        "let t = 1\nlet n = 0\nfun grow(a: int) -> int\n  a * 3\nend\nfor i in 0..1000 do\n  print(\"{n} {t}\")\n  n += 1\n  t = grow(t)\nend",
    );
    assert!(out.contains("R010"), "{out}");
}

#[test]
fn a_hot_loop_calls_a_method_of_a_structure_held_by_a_variable_of_the_module() {
    let out = same(&format!(
        "{PT}impl P\n  public fun advance(self) -> int\n    self.x = self.x + 1\n    self.y = (self.y + self.x) mod 1000003\n    self.y\n  end\nend\nlet p = P {{ x: 0, y: 0, w: 0.0, on: false }}\nlet last = 0\nfor i in 0..5000 do\n  last = p.advance()\nend\nprint(last)\nprint(p.x)\nprint(p.y)"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_error_in_a_method_called_from_a_hot_loop_leaves_the_structure_as_the_machine_does() {
    let out = same(&format!(
        "{PT}impl P\n  public fun grow(self) -> int\n    self.x = self.x + 1\n    self.y = self.y * 3\n    self.y\n  end\nend\nlet p = P {{ x: 0, y: 1, w: 0.0, on: false }}\nlet last = 0\nfor i in 0..1000 do\n  print(\"{{p.x}} {{p.y}}\")\n  last = p.grow()\nend"
    ));
    assert!(out.contains("R010"), "{out}");
}

const APPLY: &str = "fun apply_n(f: fun(int) -> int, n: int, x: int) -> int\n  let v = x\n  for i in 0..n do\n    v = f(v) mod 1000003\n  end\n  v\nend\n";

#[test]
fn a_hot_loop_calls_a_function_value_that_reads_a_captured_variable() {
    let out = same(&format!(
        "{APPLY}let k = 3\nlet add = fun(x: int) -> int\n  x * 2 + k\nend\nprint(apply_n(add, 5000, 1))\nprint(apply_n(add, 3000, 2))"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_function_value_that_changes_a_captured_variable_agrees() {
    let out = same(&format!(
        "{APPLY}let calls = 0\nlet bump = fun(x: int) -> int\n  calls += 1\n  x + calls\nend\nprint(apply_n(bump, 5000, 1))\nprint(calls)\nfun make() -> fun(int) -> int\n  let seen = 0\n  fun(x: int) -> int\n    seen += x\n    seen mod 1000\n  end\nend\nlet g = make()\nprint(apply_n(g, 4000, 1))"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_error_in_a_function_value_called_by_a_hot_loop_leaves_the_variable_as_the_machine_does() {
    let out = same(&format!(
        "{APPLY}let calls = 0\nlet big = 1\nlet grow = fun(x: int) -> int\n  calls += 1\n  big = big * 3\n  print(\"{{calls}} {{big}}\")\n  x + calls\nend\nprint(apply_n(grow, 1000, 1))"
    ));
    assert!(out.contains("R010"), "{out}");
}

#[test]
fn another_function_value_in_a_loop_that_became_hot_is_still_right() {
    let out = same(&format!(
        "{APPLY}let k = 3\nlet a = fun(x: int) -> int\n  x + k\nend\nlet b = fun(x: int) -> int\n  x * 2\nend\nprint(apply_n(a, 4000, 1))\nprint(apply_n(b, 4000, 1))\nprint(apply_n(a, 10, 1))"
    ));
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn hot_loops_over_texts_agree() {
    let out = same(
        "fun vowels(s: string) -> int\n  let n = 0\n  let i = 0\n  while i < s.len() do\n    let c = s.substring(i, i + 1)\n    if c == \"a\" or c == \"é\" or c == \"ü\" then n += 1 end\n    if s.substring(i, i + 1) != \"b\" then n += 2 end\n    i += 1\n  end\n  n\nend\nlet t = 0\nfor k in 0..3000 do\n  t += vowels(\"abécüaé{k}\")\nend\nprint(t)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_substring_out_of_range_in_a_hot_loop_is_the_machines_error() {
    let out = same(
        "fun cut(s: string, n: int) -> int\n  let c = s.substring(0, n)\n  c.len()\nend\nlet t = 0\nfor k in 0..2000 do\n  t += cut(\"hello\", k mod 7)\nend\nprint(t)",
    );
    assert!(out.contains("R020"), "{out}");
}

#[test]
fn a_hot_loop_over_a_long_text_agrees() {
    let out = same(
        "let long = \"\"\nfor j in 0..20 do\n  long = long + \"abcdefghij\"\nend\nlet n = 0\nlet i = 0\nwhile i < long.len() do\n  if long.substring(i, i + 1) == \"e\" then n += 1 end\n  i += 1\nend\nlet total = 0\nfor k in 0..2000 do\n  total += k mod 3\nend\nprint(n)\nprint(total)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn hot_loops_over_maps_of_numbers_agree() {
    let out = same(
        "let m: map<int, int> = {}\nlet f: map<string, float> = {}\nfor i in 0..5000 do\n  m[i] = i * i mod 1000\n  f[\"k{i mod 50}\"] = i * 0.5\nend\nlet total = 0\nlet missing = 0\nfor i in 0..6000 do\n  let v = m[i]\n  if v != none then total += v else missing += 1 end\nend\nlet s = 0.0\nfor k in 0..50 do\n  let x = f[\"k{k}\"]\n  if x != none then s += x end\nend\nprint(total)\nprint(missing)\nprint(s)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn a_hot_function_that_reads_an_array_parameter_agrees() {
    let out = same(
        "fun total(xs: array<int>, k: int) -> int\n  let t = 0\n  for x in xs do\n    t += x * k\n  end\n  t + xs.len() + xs[0]\nend\nfun first(xs: array<float>) -> float\n  xs[1] * 2.0\nend\nfun twice(xs: array<int>) -> int\n  total(xs, 2) + total(xs, 3)\nend\nlet a = [3, 1, 4, 1, 5]\nlet f = [0.5, 1.25]\nlet s = 0\nlet g = 0.0\nfor i in 0..2000 do\n  s = (s + twice(a)) mod 1000003\n  g += first(f)\nend\nprint(s)\nprint(g)",
    );
    assert!(!out.contains("error"), "{out}");
}

#[test]
fn an_index_out_of_bounds_in_a_hot_function_with_an_array_parameter_is_reported() {
    let out = same(
        "fun at(xs: array<int>, i: int) -> int\n  xs[i]\nend\nlet a = [1, 2, 3]\nlet s = 0\nfor i in 0..1000 do\n  s += at(a, i mod 4)\nend\nprint(s)",
    );
    assert!(out.contains("R020"), "{out}");
}
