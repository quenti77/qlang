struct P
  public x: int
  public y: int
end
impl P
  public fun advance(self) -> int
    self.x = self.x + 1
    self.y = self.y + self.x
    self.y
  end
end
let p = P { x: 0, y: 0 }
let last = 0
for i in 0..600000 do
  last = p.advance()
end
print(last)
