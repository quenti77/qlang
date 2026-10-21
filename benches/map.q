let m: map<int, int> = {}
for i in 0..200000 do
  m[i] = i * i
end
let total = 0
for i in 0..200000 do
  let v = m[i]
  if v != none then total += v end
end
print(total)
