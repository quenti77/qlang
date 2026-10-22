fun dist(x: int, y: int) -> int
  let p = [x, y]
  p[0] * p[0] + p[1] * p[1]
end
let t = 0
for i in 0..3000000 do
  t += dist(i mod 100, i mod 7)
end
print(t)
