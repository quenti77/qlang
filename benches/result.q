fun half(n: int) -> Result<int, Error>
  if n mod 2 == 1 then
    return Err(Error { message: "odd" })
  end
  Ok(n div 2)
end

fun twice(n: int) -> Result<int, Error>
  let a = half(n)?
  let b = half(a)?
  Ok(a + b)
end

let ok = 0
let bad = 0
for i in 0..200000 do
  match twice(i * 4)
    case Ok(v) then ok += v
    case Err(e) then bad += 1
  end
  match twice(i * 2 + 2)
    case Ok(v) then ok += v
    case Err(e) then bad += 1
  end
end
print(ok)
print(bad)
