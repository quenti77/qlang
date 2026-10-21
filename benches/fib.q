fun fib(n: int) -> int
  if n < 2 then n else fib(n - 1) + fib(n - 2) end
end
print(fib(27))
