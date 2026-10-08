-- all syntax in one file
import add, sub as minus from "math.q"
import "math.q" as math

--"
  documentation comment
--"
const PI = 3.14
let count = 0
let hex = 0xFF_FF + 0b0101_0101 + 1_000
let name: string? = none
let xs: array<int> = [1, 2,
  3]

struct Account
  protected balance: int = 0
  private static total: int = 0
end

struct Savings extends Account
  public rate: float = 0.02
end

trait Shape
  fun area(self) -> float
end

enum Color
  Red
  Green
end

fun sum<T: Add + Eq>(a: T, b: T) -> T
  a + b
end

impl Savings
  public fun new(id: int) -> Savings
    Savings { ..Account.new(), rate: 0.05 }
  end
  public override fun describe(self) -> string
    super.describe() + " (savings)"
  end
end

impl Add for Account
  public fun add(self, o: Account) -> Account
    o
  end
end

let f = fun(x: int) -> int
  x * 2
end

let label = match count
  case 0 then "zero"
  case 1..=9 then "small"
  case x if x > 100 then "big"
  else "other"
end

if count > 0 then print("a") elseif count < 0 then print("b") else print("c") end

for i in 0..10 step 2 do
  count += i
  if i == 4 then continue end
  if i == 8 then break end
end

while true do
  return
end

print("x = {count}, total = {count + 1} \{lit\}")
let g = sum<float>(1.0, 2.0)
let b = Box<int>.new(1)
let p = Pair<int, string> { first: 1, second: "a" }
let n = -2 ** 2 + (3 as float as int)
a[0] = b = 4
let c: (fun(int) -> int)? = none
