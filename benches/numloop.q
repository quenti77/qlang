fun work(n: int) -> int
  let total = 0
  let i = 0
  while i < n do
    total += i mod 7
    i += 1
  end
  total
end
fun sum_sq(n: int) -> float
  let acc = 0.0
  for k in 0..n do
    let x = k * 1.0
    acc += x * x
  end
  acc
end
print(work(3000000))
print(sum_sq(1000000))
