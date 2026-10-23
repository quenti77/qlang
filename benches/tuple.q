fun divmod(a: int, b: int) -> (int, int)
  (a div b, a mod b)
end

fun digit_sum(n: int) -> int
  let total = 0
  let k = n
  while k > 0 do
    let (q, r) = divmod(k, 10)
    total += r
    k = q
  end
  total
end

fun bounds(xs: array<int>) -> (int, int)
  let lo = xs[0]
  let hi = xs[0]
  for x in xs do
    if x < lo then lo = x end
    if x > hi then hi = x end
  end
  (lo, hi)
end

fun run(n: int) -> int
  let acc = 0
  for i in 0..n do
    acc += digit_sum(i * 7919)
    let (a, b) = divmod(i, 13)
    acc += a - b
  end
  acc
end

let xs = [5, 3, 9, 1, 7, 2, 8]
let total = 0
for i in 0..20000 do
  let (lo, hi) = bounds(xs)
  total += hi - lo
end
print(run(300000))
print(total)
