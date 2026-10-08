-- A tour of qlang. Run it with: qlang run examples/tour.q
import add, sub as minus from "tour_math.q"
import "tour_math.q" as math

const PI = 3.14
let count = 0
let name: string? = none
let numbers: array<int> = [3, 1, 2]

-- numbers
print(7 / 2)
print(7 div 2)
print(7 mod 2)
print(2 ** 10)
print(1 + 2.5)
print(0xFF_FF + 0b0101 + 1_000)

-- strings
let who = "world"
print("hello {who}, 1 + 2 = {1 + 2}")
print("a" + "b")

-- nullables
if name == none then
  print("no name")
end
name = "Alice"
if name != none then
  print("name has {name.len()} letters")
end

-- control flow
for i in 0..5 step 2 do
  count += i
end
print(count)
for i in 1..=3 do
  write("{i} ")
end
print("")
while count > 0 do
  count -= 3
end
print(count)

let sign = if count >= 0 then 1 else -1 end
print(sign)

-- match
fun describe(n: int) -> string
  match n
    case 0 then "zero"
    case 1..=9 then "small"
    case x if x > 100 then "huge {x}"
    else "other"
  end
end
print(describe(0))
print(describe(5))
print(describe(500))
print(describe(50))

-- functions
fun fact(n: int) -> int
  if n <= 1 then
    return 1
  end
  n * fact(n - 1)
end
print(fact(10))

fun apply(f: fun(int) -> int, x: int) -> int
  f(x)
end
let double = fun(x: int) -> int
  x * 2
end
print(apply(double, 21))

-- closures
fun counter() -> fun() -> int
  let n = 0
  fun() -> int
    n += 1
    n
  end
end
let next = counter()
next()
next()
print(next())

-- generics
fun first_of<T>(xs: array<T>) -> T?
  if xs.len() == 0 then
    return none
  end
  xs[0]
end
print(first_of(numbers))
fun biggest<T: Ord>(a: T, b: T) -> T
  if a > b then a else b end
end
print(biggest(3, 9))
print(biggest("pear", "apple"))

-- arrays
numbers[] = 10
numbers[0] = 99
print(numbers)
print(numbers.len())

-- enums
enum Color
  Red
  Green
end
let c = Color.Green
print(match c
  case Color.Red then "red"
  case Color.Green then "green"
end)

-- structs, impl, inheritance
struct Account
  protected balance: int = 0
  public owner: string
  private static created: int = 0
end

struct Savings extends Account
  public rate: float = 0.5
end

impl Account
  public fun new(owner: string) -> Account
    Account.created += 1
    Account { owner }
  end
  public fun deposit(self, amount: int)
    self.balance += amount
  end
  public fun describe(self) -> string
    "{self.owner}: {self.balance}"
  end
  public fun total_created() -> int
    Account.created
  end
end

impl Savings
  public fun open(owner: string, rate: float) -> Savings
    Savings { ..Account.new(owner), rate }
  end
  public override fun describe(self) -> string
    super.describe() + " at {self.rate}"
  end
end

let acc = Savings.open("Bob", 0.25)
acc.deposit(100)
print(acc.describe())
print(Account.total_created())

-- traits and operators
struct Point
  public x: int
  public y: int
end

impl Add for Point
  public fun add(self, o: Point) -> Point
    Point { x: self.x + o.x, y: self.y + o.y }
  end
end

impl Mul<int> for Point
  public fun mul(self, k: int) -> Point
    Point { x: self.x * k, y: self.y * k }
  end
end

impl As<string> for Point
  public fun convert(self) -> string
    "({self.x}, {self.y})"
  end
end

impl Eq for Point
  public fun eq(self, o: Point) -> bool
    self.x == o.x and self.y == o.y
  end
end

let p = Point { x: 1, y: 2 } + Point { x: 10, y: 20 }
print(p)
print(p * 3)
print(p == Point { x: 11, y: 22 })
print(p as string)

trait Shape
  fun area(self) -> float
end

struct Square
  public side: float
end

impl Shape for Square
  public fun area(self) -> float
    self.side * self.side
  end
end

fun show_area(s: Shape)
  print(s.area())
end
show_area(Square { side: 3.0 })

-- casts
print(7 as float)
print(3.9 as int)
print((2 + 3) as string + "!")
print(int.parse("42"))
print(int.parse("nope"))

-- maps
let ages = { "ana": 31, "bob": 27 }
ages["cleo"] = 45
let bob = ages["bob"]
if bob != none then
  print("bob is {bob}")
end
print(ages.get("zed", -1))
for name in ages.keys() do
  write("{name} ")
end
print("")
for name, age in ages do
  write("{name}={age} ")
end
print("")
for i, c in "abc" do
  write("{i}{c} ")
end
print("")
let rest = 17
rest div= 5
rest mod= 2
print(rest)

-- sorting and math
let scores = [42, 7, 19]
scores.sort()
print(scores)
print(scores.max())
print(scores.sum())
print(float.PI > 3.14)

-- modules
print(add(1, 2))
print(minus(5, 3))
print(math.add(10, 20))
