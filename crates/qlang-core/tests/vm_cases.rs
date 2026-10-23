//! Hand-written cases for the typed register machine: each program must give the same result
//! on the machine and on the tree walker, and the result is also checked against what it
//! should be. (`vm_differential.rs` does the same with random programs.)

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn both(src: &str, limits: Limits) -> (String, String, (usize, usize)) {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let go = |vm: bool| {
                let mut host = MemoryHost::default();
                let prog = match compile("main.q", &src, &mut host) {
                    Ok(p) => p,
                    Err(e) => return (format!("COMPILE: {}", e.render()), (0, 0)),
                };
                let r = if vm {
                    prog.run(&mut host, limits)
                } else {
                    prog.run_tree_walker(&mut host, limits)
                };
                let out = match r.error {
                    Some(d) => format!("{}{}", host.output, d.render(&prog.sources)),
                    None => host.output,
                };
                (out, r.vm_functions)
            };
            let (a, fa) = go(true);
            let (b, _) = go(false);
            (a, b, fa)
        })
        .unwrap()
        .join()
        .unwrap()
}

/// The output, after checking that both engines agree and that the machine was used.
fn same(src: &str) -> String {
    let (vm, walker, used) = both(src, Limits::default());
    assert_eq!(vm, walker, "the engines disagree on:\n{src}");
    assert!(
        used.0 > 0,
        "the machine compiled nothing in this program:\n{src}"
    );
    vm
}

#[test]
fn recursion_loops_and_mixed_numbers() {
    assert_eq!(
        same(
            "fun fib(n: int) -> int\n  if n < 2 then n else fib(n - 1) + fib(n - 2) end\nend\nprint(fib(20))"
        ),
        "6765\n"
    );
    assert_eq!(
        same(
            "fun f(n: int) -> float\n  let s = 0.0\n  for i in 1..=n do\n    s += i / 2 + 1.5\n  end\n  s\nend\nprint(f(4))"
        ),
        "11.0\n"
    );
    assert_eq!(
        same(
            "fun f(a: int, b: float) -> bool\n  a * b > 7 and (a as float) == 3.0\nend\nprint(f(3, 2.5))\nprint(f(2, 2.5))"
        ),
        "true\nfalse\n"
    );
    assert_eq!(
        same(
            "fun f(x: float) -> int\n  (x * 2.5) as int + x.floor() + x.sqrt().round()\nend\nprint(f(9.0))"
        ),
        "34\n"
    );
}

#[test]
fn errors_are_the_same_with_the_same_trace() {
    let out = same(
        "fun f(a: int) -> int\n  9223372036854775807 + a\nend\nfun g(a: int) -> int\n  f(a) * 2\nend\nprint(g(1))",
    );
    assert!(
        out.contains("R010") && out.contains("called from `f`"),
        "{out}"
    );
    let out = same("fun f(a: int) -> int\n  10 div a\nend\nprint(f(0))");
    assert!(out.contains("R011"), "{out}");
    let out = same("fun f(x: float) -> float\n  x.sqrt()\nend\nprint(f(-1.0))");
    assert!(out.contains("R014"), "{out}");
    let out = same("fun f(x: float) -> int\n  x as int\nend\nprint(f(1e300))");
    assert!(out.contains("R013"), "{out}");
    let out = same("fun f(a: int) -> int\n  2 ** a\nend\nprint(f(-1))");
    assert!(out.contains("R012"), "{out}");
    let out = same(
        "fun f(n: int) -> int\n  let t = 0\n  for i in 0..10 step n do\n    t += i\n  end\n  t\nend\nprint(f(0))",
    );
    assert!(out.contains("R031"), "{out}");
}

#[test]
fn limits_apply_inside_the_machine() {
    let (vm, walker, _) = both(
        "fun spin() -> int\n  let i = 0\n  while true do\n    i += 1\n  end\n  i\nend\nprint(spin())",
        Limits {
            max_steps: 100_000,
            ..Limits::default()
        },
    );
    assert!(vm.contains("R900") && walker.contains("R900"), "{vm}");
    let (vm, walker, _) = both(
        "fun deep(n: int) -> int\n  deep(n + 1) + 1\nend\nprint(deep(0))",
        Limits::default(),
    );
    assert!(vm.contains("R901") && walker.contains("R901"), "{vm}");
}

#[test]
fn a_loop_shares_variables_with_the_code_around_it() {
    // module variables, assigned by the loop and read afterwards
    assert_eq!(
        same(
            "let total = 0\nlet i = 0\nwhile i < 1000 do\n  total += i mod 7\n  i += 1\nend\nprint(total)\nprint(i)"
        ),
        "2997\n1000\n"
    );
    // locals of a function the machine does not compile (it prints)
    assert_eq!(
        same(
            "fun f(n: int) -> int\n  let a = 0\n  let b = 1\n  for _k in 0..n do\n    (a, b) = (b, a + b)\n  end\n  print(\"done\")\n  a\nend\nprint(f(10))"
        ),
        "done\n55\n"
    );
    // break and continue, a nested loop, a variable of the loop shadowing an outer one
    assert_eq!(
        same(
            "let x = 100\nlet s = 0\nfor i in 0..50 do\n  if i mod 2 == 0 then continue end\n  let x = i\n  for j in 0..i do\n    if j > 3 then break end\n    s += x * j\n  end\nend\nprint(s)\nprint(x)"
        ),
        "3735\n100\n"
    );
    // an inclusive range up to the biggest int does not overflow
    assert_eq!(
        same(
            "let n = 0\nfor i in 9223372036854775806..=9223372036854775807 do\n  n += 1\nend\nprint(n)"
        ),
        "2\n"
    );
    // a call inside the loop that changes a module variable the loop uses
    assert_eq!(
        same(
            "let g = 0\nfun bump(x: int) -> int\n  g += x\n  g\nend\nlet t = 0\nfor i in 1..=5 do\n  t += bump(i)\n  g += 1\nend\nprint(t)\nprint(g)"
        ),
        "45\n20\n"
    );
}

#[test]
fn what_the_machine_cannot_do_still_runs() {
    // text, arrays, structures: the tree walker, with the numeric parts around it
    assert_eq!(
        same(
            "fun sq(x: int) -> int\n  x * x\nend\nlet xs = [1, 2, 3]\nlet total = 0\nfor x in xs do\n  total += sq(x)\nend\nprint(total)\nprint(\"{sq(4)} items\")"
        ),
        "14\n16 items\n"
    );
}

/// Like `same`, and checks that at least `n` functions or loops were compiled.
fn same_compiled(src: &str, n: usize) -> String {
    let (vm, walker, used) = both(src, Limits::default());
    assert_eq!(vm, walker, "the engines disagree on:\n{src}");
    assert!(used.0 >= n, "only {} compiled, wanted {n}:\n{src}", used.0);
    vm
}

#[test]
fn arrays_of_numbers_are_worked_on_in_place() {
    assert_eq!(
        same_compiled(
            "fun fill(xs: array<int>, n: int)\n  for i in 0..n do\n    xs[] = i * i\n  end\nend\nfun total(xs: array<int>) -> int\n  let t = 0\n  for x in xs do\n    t += x\n  end\n  t\nend\nlet a: array<int> = []\nfill(a, 5)\nprint(a)\nprint(total(a))\nprint(a.len())",
            2
        ),
        "[0, 1, 4, 9, 16]\n30\n5\n"
    );
    // the same array under two names, changed through one of them
    assert_eq!(
        same_compiled(
            "fun f(xs: array<int>) -> int\n  let ys = xs\n  ys[0] = 99\n  xs[0] + ys[0]\nend\nlet a = [1, 2, 3]\nprint(f(a))\nprint(a)",
            1
        ),
        "198\n[99, 2, 3]\n"
    );
    // floats and booleans, `for i, x`, and an array given back
    assert_eq!(
        same_compiled(
            "fun f(fs: array<float>, bs: array<bool>) -> array<float>\n  for i, x in fs do\n    fs[i] = x * 2.0\n    bs[i] = x > 1.0\n  end\n  fs\nend\nlet a = [0.5, 1.5, 2.5]\nlet b = [false, false, false]\nprint(f(a, b))\nprint(b)",
            1
        ),
        "[1.0, 3.0, 5.0]\n[false, true, true]\n"
    );
    // methods of arrays through the tree walker's own implementation
    assert_eq!(
        same_compiled(
            "fun f(xs: array<int>) -> int\n  xs.sort()\n  xs.reverse()\n  xs.insert(1, 100)\n  let r = xs.remove(0)\n  r + xs.sum() + xs.len() + (if xs.contains(100) then 1 else 0 end)\nend\nlet a = [3, 1, 2]\nprint(f(a))\nprint(a)",
            1
        ),
        "110\n[100, 2, 1]\n"
    );
}

#[test]
fn array_errors_are_the_same() {
    let out = same_compiled(
        "fun f(xs: array<int>, i: int) -> int\n  xs[i]\nend\nlet a = [1, 2, 3]\nprint(f(a, 5))",
        1,
    );
    assert!(out.contains("R020") && out.contains("length 3"), "{out}");
    let out = same_compiled(
        "fun f(xs: array<int>, i: int)\n  xs[i] += 1\nend\nlet a = [1, 2, 3]\nf(a, -1)",
        1,
    );
    assert!(out.contains("R020"), "{out}");
    // the index is checked before the right side is computed
    let out = same_compiled(
        "fun f(xs: array<int>) -> int\n  xs[10] = 1 div 0\n  0\nend\nlet a = [1]\nprint(f(a))",
        1,
    );
    assert!(out.contains("R020") && !out.contains("R011"), "{out}");
    let (vm, walker, _) = both(
        "fun f(xs: array<int>)\n  for i in 0..100 do\n    xs.push(i)\n  end\nend\nlet a: array<int> = []\nf(a)",
        Limits {
            max_alloc: 10,
            ..Limits::default()
        },
    );
    assert!(vm.contains("R903") && walker.contains("R903"), "{vm}");
}

#[test]
fn a_loop_over_arrays_of_the_code_around_it() {
    assert_eq!(
        same_compiled(
            "let data: array<int> = []\nfor i in 0..1000 do\n  data[] = i mod 13\nend\nlet s = 0\nfor x in data do\n  s += x\nend\nprint(s)\nprint(data.len())",
            2
        ),
        "5994\n1000\n"
    );
}

#[test]
fn arrays_made_inside_a_function() {
    // a sieve builds its own array, a callee's array comes back, a temporary is dropped
    let src = "fun sieve(n: int) -> array<bool>\n  let s = [true, true]\n  let i = 2\n  while i <= n do\n    s.push(true)\n    i += 1\n  end\n  i = 2\n  while i * i <= n do\n    if s[i] then\n      let j = i * i\n      while j <= n do\n        s[j] = false\n        j += i\n      end\n    end\n    i += 1\n  end\n  s\nend\nfun count(n: int) -> int\n  let c = 0\n  for p in sieve(n) do\n    if p then\n      c += 1\n    end\n  end\n  c - 2\nend\nfun pair(x: int) -> int\n  let t = [x, x + 1]\n  t[0] + t[1]\nend\nlet total = 0\nfor k in 0..1000 do\n  total += pair(k)\nend\nprint(count(100))\nprint(total)";
    assert_eq!(same_compiled(src, 3), "25\n1000000\n");
    // an empty literal gets its type from the declaration
    assert_eq!(
        same(
            "fun f() -> int\n  let a: array<int> = []\n  a.push(4)\n  a.push(5)\n  a[0] * a[1]\nend\nprint(f())"
        ),
        "20\n"
    );
    // a callee's array that is returned through two calls
    assert_eq!(
        same(
            "fun a() -> array<float>\n  [1.5, 2.5]\nend\nfun b() -> array<float>\n  let x = a()\n  x.push(3.0)\n  x\nend\nprint(b().sum())"
        ),
        "7.0\n"
    );
}

#[test]
fn a_loop_with_calls_and_module_variables() {
    // the callee changes the variable the loop also changes: both see every write
    let src = "let t = 0\nfun bump(k: int)\n  t += k * 2\nend\nfun twice(k: int) -> int\n  k * 2\nend\nfor i in 0..1000 do\n  bump(i)\n  t += twice(i) mod 3\nend\nprint(t)\nlet c = 0\nlet k = 5\nwhile c < 100 do\n  c += twice(k) mod 7\n  k += 1\nend\nprint(c)\nprint(k)";
    assert_eq!(same_compiled(src, 3), "999999\n104\n39\n");
}

#[test]
fn arrays_made_in_loops_are_dropped_with_each_turn() {
    // a new array every turn: nothing piles up, and a returned one survives its call
    let src = "fun f(n: int) -> int\n  let total = 0\n  for i in 0..n do\n    let t = [i, i + 1, i + 2]\n    t.push(i)\n    total += t[0] + t[3] + t.len()\n  end\n  total\nend\nfun row(k: int) -> array<int>\n  let r: array<int> = []\n  for i in 0..k do\n    r[] = i * i\n  end\n  r\nend\nfun use_rows(n: int) -> int\n  let s = 0\n  let i = 0\n  while i < n do\n    let r = row(i mod 5)\n    for x in r do\n      s += x\n    end\n    i += 1\n  end\n  s\nend\nprint(f(200000))\nprint(use_rows(100000))";
    // the sum over i < 200000 of (i + i + 4), then 20000 turns of 0 + 0 + 1 + 5 + 14
    assert_eq!(same_compiled(src, 3), "40000600000\n400000\n");
}

#[test]
fn strings_on_the_machine() {
    let src = "fun word(i: int) -> string\n  \"w{i}\" + (if i mod 2 == 0 then \"e\" else \"o\" end)\nend\nfun longest(n: int) -> string\n  let best = \"\"\n  for i in 0..n do\n    let w = word(i * 7 mod 13)\n    if w.len() > best.len() or (w.len() == best.len() and w > best) then\n      best = w\n    end\n  end\n  best\nend\nlet text = \"\"\nfun add(t: string, k: int) -> string\n  t + \"[{k}:{k * 0.5}:{k > 1}]\"\nend\nfor k in 0..4 do\n  text = add(text, k)\nend\nprint(text)\nprint(longest(30))\nprint(\"\".is_empty())\nprint(\"Caf\u{e9}\".upper() + \"Caf\u{e9}\".len().to_hex())";
    let out = same_compiled(src, 3);
    assert_eq!(
        out,
        "[0:0.0:false][1:0.5:false][2:1.0:true][3:1.5:true]\nw12e\ntrue\nCAF\u{c9}4\n"
    );
}

#[test]
fn string_errors_and_limits_are_the_same() {
    // an index out of range inside a function the machine runs: same message, same trace
    let out = same_compiled(
        "fun cut(s: string, k: int) -> string\n  s.substring(0, k)\nend\nprint(cut(\"abc\", 2))\nprint(cut(\"abc\", 9))",
        1,
    );
    assert!(out.starts_with("ab\n") && out.contains("error[R"), "{out}");
    assert!(out.contains("called from"), "{out}");
    // a string that grows past the limit
    let (vm, walker, _) = both(
        "fun grow(n: int) -> int\n  let s = \"x\"\n  for i in 0..n do\n    s = s + s\n  end\n  s.len()\nend\nprint(grow(40))",
        Limits {
            max_alloc: 1000,
            ..Limits::default()
        },
    );
    assert!(vm.contains("R903") && walker.contains("R903"), "{vm}");
    assert_eq!(vm, walker);
}

#[test]
fn structures_and_their_methods_on_the_machine() {
    let src = "struct Acc\n  public total: int\n  public log: string = \"\"\nend\nimpl Acc\n  public fun add(self, k: int) -> int\n    self.total += k\n    self.log += \"+{k}\"\n    self.total\n  end\n  public fun boom(self) -> int\n    self.total div (self.total - self.total)\n  end\nend\nstruct Big extends Acc\n  public extra: float = 0.5\nend\nimpl Big\n  public override fun add(self, k: int) -> int\n    self.extra = self.extra * 2.0\n    self.total += k * 10\n    self.total\n  end\nend\nfun run(a: Acc, n: int) -> int\n  let t = 0\n  for i in 0..n do\n    t += a.add(i)\n  end\n  t\nend\nlet a = Acc { total: 0 }\nlet b = Big { total: 0 }\nprint(run(a, 4))\nprint(a.log)\nprint(run(b, 4))\nprint(b.extra)\nprint(a.boom())";
    let out = same_compiled(src, 2);
    assert!(out.starts_with("10\n+0+1+2+3\n"), "{out}");
    assert!(out.contains("R011"), "{out}");
    assert!(out.contains("called from `boom`"), "{out}");
}

#[test]
fn a_function_the_machine_cannot_run_is_called_through_the_interpreter() {
    // `lookup` uses a map: it stays on the tree walker, but the loop that calls it is compiled
    let src = "fun lookup(k: int) -> int\n  let m = {1: 10, 2: 20}\n  let v = m.get(k, 0)\n  v + 1\nend\nfun hot(n: int) -> int\n  let t = 0\n  for i in 0..n do\n    t += lookup(i mod 4)\n  end\n  t\nend\nprint(hot(1000))\nprint(lookup(2))";
    assert_eq!(same_compiled(src, 1), "8500\n21\n");
}

#[test]
fn a_method_nobody_overrides_is_called_directly() {
    let src = "struct A\n  public n: int\nend\nimpl A\n  public fun inc(self, k: int) -> int\n    self.n += k\n    self.n\n  end\nend\nstruct B extends A\n  public m: int = 1\nend\nfun run(a: A) -> int\n  let t = 0\n  for i in 0..100 do\n    t += a.inc(i)\n  end\n  t\nend\nprint(run(A { n: 0 }))\nprint(run(B { n: 5 }))";
    assert_eq!(same_compiled(src, 2), "166650\n167150\n");
}

#[test]
fn arrays_of_texts_structures_and_arrays() {
    let src = "struct P\n  public v: int\nend\nfun names(n: int) -> string\n  let xs: array<string> = []\n  for i in 0..n do\n    xs[] = \"n{i}\"\n  end\n  xs[0] += \"!\"\n  xs.join(\",\")\nend\nfun grid(n: int) -> int\n  let rows: array<array<int>> = []\n  for i in 0..n do\n    rows[] = [i, i * 2]\n  end\n  rows[1][1] = 50\n  let s = 0\n  for r in rows do\n    s += r[0] + r[1]\n  end\n  s\nend\nfun ps(n: int) -> int\n  let a: array<P> = []\n  for i in 0..n do\n    a.push(P { v: i })\n  end\n  let t = 0\n  for p in a do\n    p.v += 1\n    t += p.v\n  end\n  t + a[2].v\nend\nprint(names(3))\nprint(grid(4))\nprint(ps(5))\nprint(\"a b  c\".words().len())\nprint(names(2)[0])";
    let out = same_compiled(src, 3);
    assert!(out.starts_with("n0!,n1,n2\n"), "{out}");
}

#[test]
fn optional_values_on_the_machine() {
    let src = "fun parse(s: string) -> int?\n  s.to_int()\nend\nfun first_big(xs: array<int>, limit: int) -> int?\n  for x in xs do\n    if x > limit then\n      return x\n    end\n  end\n  none\nend\nfun total(words: array<string>) -> int\n  let t = 0\n  for w in words do\n    let n = parse(w)\n    if n != none then\n      t += n + 1\n    else\n      t -= 1\n    end\n  end\n  t\nend\nfun pick(flag: bool) -> string?\n  if flag then \"yes\" else none end\nend\nfun use_pick() -> int\n  let a = pick(true)\n  let b = pick(false)\n  let n = 0\n  if a != none then n += a.len() end\n  if b == none then n += 10 end\n  n\nend\nprint(total([\"1\", \"x\", \"22\", \"y\"]))\nprint(first_big([1, 5, 9], 4))\nprint(first_big([1, 2], 4))\nprint(use_pick())";
    assert_eq!(same_compiled(src, 4), "23\n5\nnone\n13\n");
}

#[test]
fn a_narrowed_variable_of_the_module_is_still_that_variable() {
    // the test narrows `y` inside the loop; the loop must go on seeing the same variable
    let src = "fun maybe(flag: bool, v: int) -> int?\n  if flag then v else none end\nend\nfun parse(s: string) -> int?\n  s.to_int()\nend\nlet acc = 0\nlet n = 1\nlet y: int? = none\ny = maybe(false, 1)\nfor i in 0..5 do\n  if y == none then n += 1 else acc += y end\n  y = parse(\"{n}\")\nend\nprint(acc)\nprint(n)\nprint(y)";
    assert_eq!(same_compiled(src, 2), "8\n2\n2\n");
}

#[test]
fn maps_on_the_machine() {
    let src = "fun counts(words: array<string>) -> map<string, int>\n  let c: map<string, int> = {}\n  for w in words do\n    c[w] = c.get(w, 0) + 1\n  end\n  c\nend\nfun best(c: map<string, int>) -> int\n  let b = 0\n  for k, v in c do\n    if v > b then b = v end\n  end\n  let missing = c[\"zzz\"]\n  if missing == none then b + c.len() else b end\nend\nfun bump(c: map<string, int>, k: string)\n  c[k] += 10\nend\nlet c = counts([\"a\", \"b\", \"a\", \"c\", \"a\"])\nprint(best(c))\nbump(c, \"b\")\nprint(c)\nlet lit = {1: \"x\", 2: \"y\"}\nlet t = \"\"\nfor k, v in lit do\n  t += \"{k}{v}\"\nend\nprint(t)\nbump(c, \"nope\")";
    let out = same_compiled(src, 3);
    assert!(
        out.starts_with("6\n{\"a\": 3, \"b\": 11, \"c\": 1}\n1x2y\n"),
        "{out}"
    );
    assert!(
        out.contains("R022") && out.contains("called from `bump`"),
        "{out}"
    );
}
