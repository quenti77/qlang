let parts: array<string> = []
for i in 0..100000 do
  parts[] = "item-{i}"
end
let joined = parts.join(",")
print(joined.len())
let n = 0
for s in parts do
  if s.contains("9") then n += 1 end
end
print(n)
