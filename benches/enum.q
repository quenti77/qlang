enum Shape
  Circle(r: float)
  Rect(w: float, h: float)
  Empty
end

fun area(s: Shape) -> float
  match s
    case Circle(r) then 3.14159 * r * r
    case Rect(w, h) then w * h
    case Empty then 0.0
  end
end

fun make(i: int) -> Shape
  match i mod 3
    case 0 then Shape.Circle(1.0 * (i mod 7))
    case 1 then Shape.Rect(1.0 * (i mod 5), 2.0)
    else Shape.Empty
  end
end

fun classify(n: int) -> int
  match n
    case 0 then 0
    case 1..10 then 1
    case 10..=100 then 2
    case x if x < 0 then 3
    else 4
  end
end

let total = 0.0
let k = 0
for i in 0..300000 do
  total += area(make(i))
  k += classify(i mod 150 - 20)
end
print(total)
print(k)
