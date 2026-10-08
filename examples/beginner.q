-- fizzbuzz
for i in 1..=15 do
  if i mod 15 == 0 then
    write("FizzBuzz ")
  elseif i mod 3 == 0 then
    write("Fizz ")
  elseif i mod 5 == 0 then
    write("Buzz ")
  else
    write("{i} ")
  end
end
print("")

-- bubble sort
fun bubble_sort(xs: array<int>)
  let n = xs.len()
  for i in 0..n do
    for j in 0..(n - i - 1) do
      if xs[j] > xs[j + 1] then
        let tmp = xs[j]
        xs[j] = xs[j + 1]
        xs[j + 1] = tmp
      end
    end
  end
end
let data = [5, 2, 9, 1, 7]
bubble_sort(data)
print(data)

-- linked list
struct Node
  public value: int
  public next: Node?
end
let head = Node { value: 1, next: Node { value: 2, next: Node { value: 3, next: none } } }
let cur: Node? = head
let total = 0
while cur != none do
  total += cur.value
  cur = cur.next
end
print(total)

-- reverse a string
fun reverse(s: string) -> string
  let out = ""
  for c in s do
    out = c + out
  end
  out
end
print(reverse("qlang"))

-- guessing with read-like parse
fun to_celsius(f: float) -> float
  (f - 32) * 5 / 9
end
print(to_celsius(212.0))
print(to_celsius(32.0))

-- average
let scores = [12, 15, 9]
let sum = 0
for s in scores do
  sum += s
end
print("average = {sum / scores.len()}")

-- gcd
fun gcd(a: int, b: int) -> int
  if b == 0 then a else gcd(b, a mod b) end
end
print(gcd(48, 18))

-- factorial with while
let n = 5
let f = 1
while n > 1 do
  f *= n
  n -= 1
end
print(f)

-- counting words
let words = "the quick brown fox jumps over the lazy dog".split(" ")
let seen: array<string> = []
let counts: array<int> = []
for w in words do
  let idx = seen.index_of(w)
  if idx == none then
    seen[] = w
    counts[] = 1
  else
    counts[idx] += 1
  end
end
print(seen.len())
let k = seen.index_of("the")
if k != none then
  print(counts[k])
end
