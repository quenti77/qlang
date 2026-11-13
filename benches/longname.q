-- The same recursion as fib.q with a long function name: a name that does not fit the
-- small-string buffer must not cost an allocation at every call.
fun calculer_la_suite_de_fibonacci_recursive(n: int) -> int
  if n < 2 then return n end
  calculer_la_suite_de_fibonacci_recursive(n - 1) + calculer_la_suite_de_fibonacci_recursive(n - 2)
end
print(calculer_la_suite_de_fibonacci_recursive(30))
