let xs: array<int> = []
for i in 0..400000 do
  xs[] = i * 2
end
let sum = 0
for i in 0..xs.len() do
  sum += xs[i]
end
print(sum)
