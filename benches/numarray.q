fun sieve(flags: array<bool>, n: int) -> int
  let count = 0
  for i in 2..n do
    if flags[i] then
      count += 1
      let j = i * i
      while j < n do
        flags[j] = false
        j += i
      end
    end
  end
  count
end

fun prefix(xs: array<int>) -> int
  let acc = 0
  for i in 0..xs.len() do
    acc += xs[i]
    xs[i] = acc
  end
  xs[xs.len() - 1]
end

let n = 2000000
let flags: array<bool> = []
for i in 0..n do
  flags.push(true)
end
print(sieve(flags, n))

let xs: array<int> = []
for i in 0..1000000 do
  xs[] = i mod 10
end
print(prefix(xs))
let s = 0
for x in xs do
  s += x mod 7
end
print(s)
