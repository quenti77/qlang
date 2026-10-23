fun count_vowels(s: string) -> int
  let n = 0
  let i = 0
  while i < s.len() do
    let c = s.substring(i, i + 1)
    if c == "a" or c == "e" or c == "i" or c == "o" or c == "u" then
      n += 1
    end
    i += 1
  end
  n
end

fun label(i: int) -> string
  "item-{i}-" + (if i mod 2 == 0 then "even" else "odd" end)
end

let total = 0
for k in 0..200000 do
  total += count_vowels(label(k))
end
print(total)
