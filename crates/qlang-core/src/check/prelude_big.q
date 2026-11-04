
-- ===== big numbers =====

--" An integer without limit (an `int` stops at about 9.2e18: past that, the program stops with an overflow).
Write `BigInt.of(12)`, `BigInt.parse("123456789012345678901234567890")` or `BigInt.pow(2, 200)`; the operators
`+ - * ** div %` work on two `BigInt`s and between a `BigInt` and an `int` (`BigInt.of(3) + 2`); the
comparisons `== < >` need two `BigInt`s (`n == BigInt.of(5)`). `/` is not defined on a `BigInt`: use `div` for the whole quotient (cut toward 0, like
`int`), or `BigDecimal`. `x as string` and `"{x}"` give the digits. Nothing here is native: it is written in
qlang, fine for a factorial of a few thousand, slow for much more. --"
struct BigInt
    --" `true` for a negative number (zero is never negative). --"
    public negative: bool
    --" The digits in base 1000000000, the least significant first, with no zero at the end (zero is `[]`). --"
    public limbs: array<int>
end

fun big_trim(limbs: array<int>) -> array<int>
    let n = limbs.len()
    while n > 0 and limbs[n - 1] == 0 do
        n -= 1
    end
    if n == limbs.len() then
        return limbs
    end
    let out: array<int> = []
    for i in 0..n do
        out.push(limbs[i])
    end
    out
end

fun big_cmp_mag(a: array<int>, b: array<int>) -> int
    if a.len() != b.len() then
        return if a.len() < b.len() then -1 else 1 end
    end
    let i = a.len() - 1
    while i >= 0 do
        if a[i] != b[i] then
            return if a[i] < b[i] then -1 else 1 end
        end
        i -= 1
    end
    0
end

fun big_add_mag(a: array<int>, b: array<int>) -> array<int>
    let out: array<int> = []
    let carry = 0
    let i = 0
    let n = if a.len() > b.len() then a.len() else b.len() end
    while i < n or carry > 0 do
        let s = carry
        if i < a.len() then
            s += a[i]
        end
        if i < b.len() then
            s += b[i]
        end
        if s >= 1000000000 then
            out.push(s - 1000000000)
            carry = 1
        else
            out.push(s)
            carry = 0
        end
        i += 1
    end
    out
end

-- a - b, with a >= b
fun big_sub_mag(a: array<int>, b: array<int>) -> array<int>
    let out: array<int> = []
    let borrow = 0
    for i in 0..a.len() do
        let d = a[i] - borrow
        if i < b.len() then
            d -= b[i]
        end
        if d < 0 then
            d += 1000000000
            borrow = 1
        else
            borrow = 0
        end
        out.push(d)
    end
    big_trim(out)
end

fun big_mul_small(a: array<int>, m: int) -> array<int>
    if m == 0 or a.is_empty() then
        return []
    end
    let out: array<int> = []
    let carry = 0
    for x in a do
        let cur = x * m + carry
        out.push(cur % 1000000000)
        carry = cur div 1000000000
    end
    if carry > 0 then
        out.push(carry)
    end
    out
end

fun big_mul_mag(a: array<int>, b: array<int>) -> array<int>
    if a.is_empty() or b.is_empty() then
        return []
    end
    let out: array<int> = []
    for k in 0..(a.len() + b.len()) do
        out.push(0)
    end
    for i in 0..a.len() do
        let carry = 0
        for j in 0..b.len() do
            let cur = out[i + j] + a[i] * b[j] + carry
            out[i + j] = cur % 1000000000
            carry = cur div 1000000000
        end
        let k = i + b.len()
        while carry > 0 do
            let cur = out[k] + carry
            out[k] = cur % 1000000000
            carry = cur div 1000000000
            k += 1
        end
    end
    big_trim(out)
end

-- (quotient, remainder) of a by a number smaller than 1000000000
fun big_divmod_small(a: array<int>, d: int) -> (array<int>, int)
    let q: array<int> = []
    for k in 0..a.len() do
        q.push(0)
    end
    let rem = 0
    let i = a.len() - 1
    while i >= 0 do
        let cur = rem * 1000000000 + a[i]
        q[i] = cur div d
        rem = cur % d
        i -= 1
    end
    (big_trim(q), rem)
end

-- (quotient, remainder) of magnitudes, b not zero
fun big_divmod_mag(a: array<int>, b: array<int>) -> (array<int>, array<int>)
    if big_cmp_mag(a, b) < 0 then
        return ([], a)
    end
    if b.len() == 1 then
        let r = big_divmod_small(a, b[0])
        let rem: array<int> = if r.1 == 0 then [] else [r.1] end
        return (r.0, rem)
    end
    let q: array<int> = []
    for k in 0..a.len() do
        q.push(0)
    end
    let rem: array<int> = []
    let i = a.len() - 1
    while i >= 0 do
        -- rem = rem * BASE + a[i]
        let shifted: array<int> = [a[i]]
        for x in rem do
            shifted.push(x)
        end
        rem = big_trim(shifted)
        -- the biggest digit d with b * d <= rem, by bisection
        let lo = 0
        let hi = 999999999
        while lo < hi do
            let mid = (lo + hi + 1) div 2
            if big_cmp_mag(big_mul_small(b, mid), rem) <= 0 then
                lo = mid
            else
                hi = mid - 1
            end
        end
        q[i] = lo
        if lo > 0 then
            rem = big_sub_mag(rem, big_mul_small(b, lo))
        end
        i -= 1
    end
    (big_trim(q), rem)
end

fun big_make(negative: bool, limbs: array<int>) -> BigInt
    let t = big_trim(limbs)
    BigInt { negative: negative and not t.is_empty(), limbs: t }
end

impl BigInt
    --" The `BigInt` of an `int`. --"
    public fun of(n: int) -> BigInt
        let limbs: array<int> = []
        let v = n
        -- digits are taken from the negative side so that the smallest `int` works too
        if v > 0 then
            v = -v
        end
        while v != 0 do
            limbs.push(-(v % 1000000000))
            v = v div 1000000000
        end
        BigInt { negative: n < 0, limbs: limbs }
    end

    --" Reads an integer written in decimal digits, with an optional `+` or `-`; anything else is an `err`. --"
    public fun parse(text: string) -> Result<BigInt, Error>
        let digits = text
        let negative = false
        if digits.starts_with("-") then
            negative = true
            digits = digits.substring(1, digits.len())
        elseif digits.starts_with("+") then
            digits = digits.substring(1, digits.len())
        end
        if digits == "" or not digits.is_digit() then
            return Result<BigInt, Error>.Err(Error { message: "not an integer: \"" + text + "\"" })
        end
        let limbs: array<int> = []
        let upto = digits.len()
        while upto > 0 do
            let from_at = if upto > 9 then upto - 9 else 0 end
            limbs.push(digits.substring(from_at, upto).to_int() ?? 0)
            upto = from_at
        end
        Result<BigInt, Error>.Ok(big_make(negative, limbs))
    end

    --" `base` to the power `exponent` (not negative) as a `BigInt`: `BigInt.pow(2, 200)`. --"
    public fun pow(base: int, exponent: int) -> BigInt
        BigInt.of(base).power(exponent)
    end

    --" Zero. --"
    public fun zero() -> BigInt
        BigInt { negative: false, limbs: [] }
    end

    --" One. --"
    public fun one() -> BigInt
        BigInt { negative: false, limbs: [1] }
    end

    --" Whether the number is zero. --"
    public fun is_zero(self) -> bool
        self.limbs.is_empty()
    end

    --" -1, 0 or 1. --"
    public fun sign(self) -> int
        if self.limbs.is_empty() then 0 elseif self.negative then -1 else 1 end
    end

    --" The same number without its sign. --"
    public fun abs(self) -> BigInt
        BigInt { negative: false, limbs: self.limbs }
    end

    --" The opposite number. --"
    public fun negate(self) -> BigInt
        big_make(not self.negative, self.limbs)
    end

    --" -1, 0 or 1 as `self` is smaller than, equal to or bigger than `other`. --"
    public fun compare(self, other: BigInt) -> int
        if self.negative != other.negative then
            return if self.negative then -1 else 1 end
        end
        let c = big_cmp_mag(self.limbs, other.limbs)
        if self.negative then -c else c end
    end

    --" The sum. --"
    public fun plus(self, other: BigInt) -> BigInt
        if self.negative == other.negative then
            return big_make(self.negative, big_add_mag(self.limbs, other.limbs))
        end
        let c = big_cmp_mag(self.limbs, other.limbs)
        if c == 0 then
            return BigInt.zero()
        end
        if c > 0 then
            return big_make(self.negative, big_sub_mag(self.limbs, other.limbs))
        end
        big_make(other.negative, big_sub_mag(other.limbs, self.limbs))
    end

    --" The difference. --"
    public fun minus(self, other: BigInt) -> BigInt
        self.plus(other.negate())
    end

    --" The product. --"
    public fun times(self, other: BigInt) -> BigInt
        big_make(self.negative != other.negative, big_mul_mag(self.limbs, other.limbs))
    end

    --" The quotient and the remainder as a pair, the quotient cut toward 0 and the remainder with the sign of
    `self` (like `div` and `%` of an `int`). Stops the program if `other` is zero. --"
    public fun divmod(self, other: BigInt) -> (BigInt, BigInt)
        if other.limbs.is_empty() then
            panic("division by zero")
        end
        let r = big_divmod_mag(self.limbs, other.limbs)
        (big_make(self.negative != other.negative, r.0), big_make(self.negative, r.1))
    end

    --" The whole quotient (cut toward 0). --"
    public fun quotient(self, other: BigInt) -> BigInt
        self.divmod(other).0
    end

    --" The remainder, with the sign of `self`. --"
    public fun remainder(self, other: BigInt) -> BigInt
        self.divmod(other).1
    end

    --" The power `self` to `exponent` (not negative, an `int`). --"
    public fun power(self, exponent: int) -> BigInt
        if exponent < 0 then
            panic("BigInt.power needs a exponent that is not negative")
        end
        let result = BigInt.one()
        let base = self
        let e = exponent
        while e > 0 do
            if e % 2 == 1 then
                result = result.times(base)
            end
            e = e div 2
            if e > 0 then
                base = base.times(base)
            end
        end
        result
    end

    --" The number as an `int`, or `err` when it does not fit. --"
    public fun to_int(self) -> Result<int, Error>
        let limit = big_cmp_mag(self.limbs, [854775808, 223372036, 9])
        if limit > 0 or (limit == 0 and not self.negative) then
            return Result<int, Error>.Err(Error { message: "the number does not fit in an int" })
        end
        -- accumulated on the negative side: its range is the bigger one
        let v = 0
        let i = self.limbs.len() - 1
        while i >= 0 do
            v = v * 1000000000 - self.limbs[i]
            i -= 1
        end
        Result<int, Error>.Ok(if self.negative then v else -v end)
    end

    --" The digits, with a `-` for a negative number. --"
    public fun to_string(self) -> string
        if self.limbs.is_empty() then
            return "0"
        end
        let out = if self.negative then "-" else "" end
        let i = self.limbs.len() - 1
        out += "{self.limbs[i]}"
        i -= 1
        while i >= 0 do
            out += "{self.limbs[i]}".pad_left(9, "0")
            i -= 1
        end
        out
    end
end

impl As<string> for BigInt
    public fun convert(self) -> string
        self.to_string()
    end
end

impl Eq for BigInt
    public fun eq(self, other: BigInt) -> bool
        self.negative == other.negative and big_cmp_mag(self.limbs, other.limbs) == 0
    end
end

impl Ord for BigInt
    public fun cmp(self, other: BigInt) -> int
        self.compare(other)
    end
end

impl Add for BigInt
    public fun add(self, other: BigInt) -> BigInt
        self.plus(other)
    end
end

impl Sub for BigInt
    public fun sub(self, other: BigInt) -> BigInt
        self.minus(other)
    end
end

impl Mul for BigInt
    public fun mul(self, other: BigInt) -> BigInt
        self.times(other)
    end
end

impl IntDiv for BigInt
    public fun int_div(self, other: BigInt) -> BigInt
        self.quotient(other)
    end
end

impl Mod for BigInt
    public fun modulo(self, other: BigInt) -> BigInt
        self.remainder(other)
    end
end

impl Neg for BigInt
    public fun neg(self) -> BigInt
        self.negate()
    end
end

impl Pow<int> for BigInt
    public fun pow(self, exponent: int) -> BigInt
        self.power(exponent)
    end
end

impl Add<int> for BigInt
    public fun add(self, other: int) -> BigInt
        self.plus(BigInt.of(other))
    end
end

impl Sub<int> for BigInt
    public fun sub(self, other: int) -> BigInt
        self.minus(BigInt.of(other))
    end
end

impl Mul<int> for BigInt
    public fun mul(self, other: int) -> BigInt
        self.times(BigInt.of(other))
    end
end

impl IntDiv<int> for BigInt
    public fun int_div(self, other: int) -> BigInt
        self.quotient(BigInt.of(other))
    end
end

impl Mod<int> for BigInt
    public fun modulo(self, other: int) -> BigInt
        self.remainder(BigInt.of(other))
    end
end

impl Add<BigInt> for int
    public fun add(self, other: BigInt) -> BigInt
        BigInt.of(self).plus(other)
    end
end

impl Sub<BigInt> for int
    public fun sub(self, other: BigInt) -> BigInt
        BigInt.of(self).minus(other)
    end
end

impl Mul<BigInt> for int
    public fun mul(self, other: BigInt) -> BigInt
        BigInt.of(self).times(other)
    end
end

--" An exact decimal number, for money: `BigDecimal.parse("12.50")`, `BigDecimal.of(3)`. It is a `BigInt` and a
**scale** (the number of digits after the point): `12.50` is `1250` with scale 2, and it prints as `12.50`. `+ - *`
are exact (the scale of a product is the sum of the scales); `==` and `<` compare the values (`12.50 == 12.5`).
`/` is not defined: a division needs the scale and the rounding wanted, so write `a.divide(b, 2, "half_even")`.
The roundings are `"half_even"` (the banker's one), `"half_up"`, `"half_down"`, `"up"` (away from 0), `"down"`
(toward 0), `"floor"` and `"ceiling"`. Comparing with a `float` is not possible, which is the point. --"
struct BigDecimal
    --" The digits without the point. --"
    public unscaled: BigInt
    --" How many of them are after the point (not negative). --"
    public scale: int
end

fun big_pow10(n: int) -> BigInt
    BigInt.pow(10, n)
end

-- the quotient of num by den (den not zero) rounded as the mode says
fun big_round_div(num: BigInt, den: BigInt, mode: string) -> BigInt
    let negative = num.negative != den.negative
    let qr = big_divmod_mag(num.limbs, den.limbs)
    let q = big_make(false, qr.0)
    let r = big_make(false, qr.1)
    if r.is_zero() then
        return big_make(negative, q.limbs)
    end
    let twice = r.times(BigInt.of(2)).compare(big_make(false, den.limbs))
    let odd = not q.is_zero() and q.limbs[0] % 2 == 1
    let up = false
    if mode == "up" then
        up = true
    elseif mode == "down" then
        up = false
    elseif mode == "floor" then
        up = negative
    elseif mode == "ceiling" then
        up = not negative
    elseif mode == "half_up" then
        up = twice >= 0
    elseif mode == "half_down" then
        up = twice > 0
    elseif mode == "half_even" then
        up = twice > 0 or (twice == 0 and odd)
    else
        panic("unknown rounding \"" + mode + "\": half_even, half_up, half_down, up, down, floor or ceiling")
    end
    let rounded = if up then q.plus(BigInt.one()) else q end
    big_make(negative, rounded.limbs)
end

impl BigDecimal
    --" The `BigDecimal` of an `int`, with scale 0. --"
    public fun of(n: int) -> BigDecimal
        BigDecimal { unscaled: BigInt.of(n), scale: 0 }
    end

    --" The `BigDecimal` of a `BigInt`, with scale 0. --"
    public fun from_bigint(n: BigInt) -> BigDecimal
        BigDecimal { unscaled: n, scale: 0 }
    end

    --" `unscaled` and the number of digits after the point: `BigDecimal.from_parts(BigInt.of(1250), 2)` is `12.50`. --"
    public fun from_parts(unscaled: BigInt, scale: int) -> BigDecimal
        if scale < 0 then
            panic("a scale is not negative")
        end
        BigDecimal { unscaled: unscaled, scale: scale }
    end

    --" Reads `12.50`, `-0.5`, `7` (digits, at most one point, an optional sign); anything else is an `err`.
    The scale is the number of digits after the point. --"
    public fun parse(text: string) -> Result<BigDecimal, Error>
        let parts = text.split(".")
        if parts.len() > 2 then
            return Result<BigDecimal, Error>.Err(Error { message: "not a decimal number: \"" + text + "\"" })
        end
        let whole = parts[0]
        let fraction = if parts.len() == 2 then parts[1] else "" end
        if parts.len() == 2 and (fraction == "" or not fraction.is_digit()) then
            return Result<BigDecimal, Error>.Err(Error { message: "not a decimal number: \"" + text + "\"" })
        end
        let n = BigInt.parse(whole + fraction)
        if n.is_err() then
            return Result<BigDecimal, Error>.Err(Error { message: "not a decimal number: \"" + text + "\"" })
        end
        Result<BigDecimal, Error>.Ok(BigDecimal { unscaled: n.unwrap(), scale: fraction.len() })
    end

    --" -1, 0 or 1. --"
    public fun sign(self) -> int
        self.unscaled.sign()
    end

    --" Whether the value is zero. --"
    public fun is_zero(self) -> bool
        self.unscaled.is_zero()
    end

    --" The same value without its sign. --"
    public fun abs(self) -> BigDecimal
        BigDecimal { unscaled: self.unscaled.abs(), scale: self.scale }
    end

    --" The opposite value. --"
    public fun negate(self) -> BigDecimal
        BigDecimal { unscaled: self.unscaled.negate(), scale: self.scale }
    end

    -- the unscaled value brought to a bigger scale
    public fun widened(self, scale: int) -> BigInt
        if scale <= self.scale then
            return self.unscaled
        end
        self.unscaled.times(big_pow10(scale - self.scale))
    end

    --" -1, 0 or 1 as the value is smaller than, equal to or bigger than `other`'s. --"
    public fun compare(self, other: BigDecimal) -> int
        let scale = if self.scale > other.scale then self.scale else other.scale end
        self.widened(scale).compare(other.widened(scale))
    end

    --" The exact sum (the bigger of the two scales). --"
    public fun plus(self, other: BigDecimal) -> BigDecimal
        let scale = if self.scale > other.scale then self.scale else other.scale end
        BigDecimal { unscaled: self.widened(scale).plus(other.widened(scale)), scale: scale }
    end

    --" The exact difference. --"
    public fun minus(self, other: BigDecimal) -> BigDecimal
        self.plus(other.negate())
    end

    --" The exact product (the scale is the sum of the scales). --"
    public fun times(self, other: BigDecimal) -> BigDecimal
        BigDecimal { unscaled: self.unscaled.times(other.unscaled), scale: self.scale + other.scale }
    end

    --" The quotient with `scale` digits after the point, rounded as `mode` says (`"half_even"` is the usual
    choice). Stops the program if `other` is zero. --"
    public fun divide(self, other: BigDecimal, scale: int, mode: string) -> BigDecimal
        if other.is_zero() then
            panic("division by zero")
        end
        let e = other.scale + scale - self.scale
        let num = if e >= 0 then self.unscaled.times(big_pow10(e)) else self.unscaled end
        let den = if e >= 0 then other.unscaled else other.unscaled.times(big_pow10(-e)) end
        BigDecimal { unscaled: big_round_div(num, den, mode), scale: scale }
    end

    --" The same value with `scale` digits after the point: more digits add zeros, fewer round as `mode` says. --"
    public fun round(self, scale: int, mode: string) -> BigDecimal
        if scale >= self.scale then
            return BigDecimal { unscaled: self.widened(scale), scale: scale }
        end
        BigDecimal { unscaled: big_round_div(self.unscaled, big_pow10(self.scale - scale), mode), scale: scale }
    end

    --" The integer part as a `BigInt`, rounded as `mode` says (`"down"` cuts the decimals). --"
    public fun to_bigint(self, mode: string) -> BigInt
        self.round(0, mode).unscaled
    end

    --" The nearest `float` (it loses the exactness: on purpose, the only way to a `float`). --"
    public fun to_float(self) -> float
        (self.to_string().to_float() ?? 0.0)
    end

    --" The same value with the trailing zeros of the decimals removed (`12.50` becomes `12.5`). --"
    public fun strip_zeros(self) -> BigDecimal
        let u = self.unscaled
        let s = self.scale
        while s > 0 and not u.is_zero() and u.remainder(BigInt.of(10)).is_zero() do
            u = u.quotient(BigInt.of(10))
            s -= 1
        end
        if u.is_zero() then
            return BigDecimal { unscaled: u, scale: 0 }
        end
        BigDecimal { unscaled: u, scale: s }
    end

    --" The digits with the point where the scale says: `12.50`. --"
    public fun to_string(self) -> string
        let digits = self.unscaled.abs().to_string()
        let sign = if self.unscaled.negative then "-" else "" end
        if self.scale == 0 then
            return sign + digits
        end
        let padded = digits.pad_left(self.scale + 1, "0")
        let cut = padded.len() - self.scale
        sign + padded.substring(0, cut) + "." + padded.substring(cut, padded.len())
    end
end

impl As<string> for BigDecimal
    public fun convert(self) -> string
        self.to_string()
    end
end

impl Eq for BigDecimal
    public fun eq(self, other: BigDecimal) -> bool
        self.compare(other) == 0
    end
end

impl Ord for BigDecimal
    public fun cmp(self, other: BigDecimal) -> int
        self.compare(other)
    end
end

impl Add for BigDecimal
    public fun add(self, other: BigDecimal) -> BigDecimal
        self.plus(other)
    end
end

impl Sub for BigDecimal
    public fun sub(self, other: BigDecimal) -> BigDecimal
        self.minus(other)
    end
end

impl Mul for BigDecimal
    public fun mul(self, other: BigDecimal) -> BigDecimal
        self.times(other)
    end
end

impl Neg for BigDecimal
    public fun neg(self) -> BigDecimal
        self.negate()
    end
end

impl Add<int> for BigDecimal
    public fun add(self, other: int) -> BigDecimal
        self.plus(BigDecimal.of(other))
    end
end

impl Sub<int> for BigDecimal
    public fun sub(self, other: int) -> BigDecimal
        self.minus(BigDecimal.of(other))
    end
end

impl Mul<int> for BigDecimal
    public fun mul(self, other: int) -> BigDecimal
        self.times(BigDecimal.of(other))
    end
end

impl Add<BigDecimal> for int
    public fun add(self, other: BigDecimal) -> BigDecimal
        BigDecimal.of(self).plus(other)
    end
end

impl Sub<BigDecimal> for int
    public fun sub(self, other: BigDecimal) -> BigDecimal
        BigDecimal.of(self).minus(other)
    end
end

impl Mul<BigDecimal> for int
    public fun mul(self, other: BigDecimal) -> BigDecimal
        BigDecimal.of(self).times(other)
    end
end

impl ToJson for BigInt
    --" A JSON string (a number would lose digits in many readers). --"
    public fun to_json(self) -> string
        json_escape(self.to_string())
    end
end

impl ToJson for BigDecimal
    --" A JSON string, scale kept (`"12.50"`). --"
    public fun to_json(self) -> string
        json_escape(self.to_string())
    end
end
