let seen = Set<int>.new()
let dup = 0
for i in 0..200000 do
  if not seen.add(i mod 5000) then dup += 1 end
end
let total = 0
for v in seen do
  total += v
end
print(dup)
print(total)
print(seen.contains(42))
