let calls = 0

fun apply_n(f: fun(int) -> int, n: int, x: int) -> int
  let v = x
  for i in 0..n do
    v = f(v) mod 1000003
  end
  v
end

fun fold(xs: array<int>, init: int, f: fun(int, int) -> int) -> int
  let acc = init
  for x in xs do
    acc = f(acc, x)
  end
  acc
end

fun count_if(xs: array<int>, pred: fun(int) -> bool) -> int
  let n = 0
  for x in xs do
    if pred(x) then n += 1 end
  end
  n
end

let k = 3
let bump = fun(x: int) -> int
  calls += 1
  x * 2 + k
end
let xs: array<int> = []
for i in 0..200000 do
  xs.push(i)
end
print(apply_n(bump, 1000000, 1))
print(fold(xs, 0, fun(a: int, b: int) -> int
  (a + b * k) mod 1000003
end))
print(count_if(xs, fun(x: int) -> bool
  x mod 7 == 0
end))
print(calls)
