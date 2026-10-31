trait Container<T>
  fun get(self, i: int) -> T
  fun size(self) -> int
end

struct Stack<T>
  private items: array<T>
end

impl<T> Stack<T>
  public fun new() -> Stack<T>
    Stack { items: [] }
  end
  public fun push(self, v: T)
    self.items[] = v
  end
  public fun pop(self) -> T?
    self.items.pop()
  end
end

impl<T> Container<T> for Stack<T>
  public fun get(self, i: int) -> T
    self.items[i]
  end
  public fun size(self) -> int
    self.items.len()
  end
end

fun last<T, C: Container<T>>(c: C) -> T
  c.get(c.size() - 1)
end

let s = Stack<int>.new()
s.push(10)
s.push(20)
print(last(s))
print(s.pop())
print(s.size())

-- generic function with function parameter
fun map<T, U>(xs: array<T>, f: fun(T) -> U) -> array<U>
  let out: array<U> = []
  for x in xs do
    out[] = f(x)
  end
  out
end
print(map([1, 2, 3], fun(x: int) -> string
  "n{x}"
end))

fun filter<T>(xs: array<T>, keep: fun(T) -> bool) -> array<T>
  let out: array<T> = []
  for x in xs do
    if keep(x) then
      out[] = x
    end
  end
  out
end
print(filter([1, 2, 3, 4, 5, 6], fun(x: int) -> bool
  x mod 2 == 0
end))

fun reduce<T, A>(xs: array<T>, init: A, f: fun(A, T) -> A) -> A
  let acc = init
  for x in xs do
    acc = f(acc, x)
  end
  acc
end
print(reduce([1, 2, 3, 4], 0, fun(a: int, x: int) -> int
  a + x
end))

-- struct with nullable field + method returning nullable
struct User
  public name: string
  public email: string?
end
impl User
  public fun contact(self) -> string
    let e = self.email
    if e == none then
      return "no email for {self.name}"
    end
    "{self.name} <{e}>"
  end
end
print(User { name: "Ann", email: none }.contact())
print(User { name: "Bob", email: "b@x.io" }.contact())

-- shadowing and scopes
let x = 1
if true then
  let x = 2
  print(x)
end
print(x)

-- nested closures
fun adder(n: int) -> fun(int) -> fun(int) -> int
  fun(a: int) -> fun(int) -> int
    fun(b: int) -> int
      n + a + b
    end
  end
end
print(adder(1)(2)(3))

-- equality of nullables and enums in arrays
enum E
  A
  B
end
print([E.A, E.B] == [E.A, E.B])
print([1, 2, 3] == [1, 2])
let m: int? = 4
print(m == 4)
-- chained methods
print("  Hello World ".trim().lower().replace("o", "0").split(" "))
