//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod annotations;
mod body;
mod calls;
mod captured;
pub mod catalog;
mod decls;
mod exhaust;
mod expr;
mod info;
mod unify;

pub use info::{ExprInfo, Info, ItemInfo, LocalInfo, Member, RefInfo};

use crate::ast::{self, Item};
use crate::diag::{Diagnostic, MAX_DIAGNOSTICS, Severity};
use crate::host::{Capabilities, Host};
use crate::parser::parse_module_from;
use crate::span::{SourceMap, Span, resolve_module_path};
use crate::types::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct ModuleData {
    pub path: String,
    pub file: u32,
    pub ast: Rc<ast::Module>,
}

/// A checked program: sources, definitions and what each name refers to.
pub struct Program {
    pub sources: SourceMap,
    /// In execution order: a module comes after the modules it imports.
    pub modules: Vec<ModuleData>,
    pub defs: Defs,
    pub res: Resolutions,
    pub warnings: Vec<Diagnostic>,
    /// For editors; only when asked for (see [`CompileOptions`]).
    pub info: Option<Info>,
}

/// Compilation failed. Diagnostics carry resolved locations.
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
    /// What the checker learned before stopping (only when asked for).
    pub info: Option<Box<Info>>,
}

impl CompileError {
    pub fn render(&self) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render(&self.sources))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Compile the program whose entry file is `entry` with text `source`.
/// Imported modules are requested from `host`.
pub fn compile(entry: &str, source: &str, host: &mut dyn Host) -> Result<Program, CompileError> {
    compile_with(entry, source, host, &CompileOptions::default())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompileOptions {
    /// Also collect the types of expressions, local variables and members (for editors).
    pub info: bool,
}

/// Like [`compile`], with options.
pub fn compile_with(
    entry: &str,
    source: &str,
    host: &mut dyn Host,
    options: &CompileOptions,
) -> Result<Program, CompileError> {
    // A bug in the compiler must not take the caller down with it (a server, an editor).
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_inner(entry, source, host, options)
    })) {
        Ok(result) => result,
        Err(payload) => Err(CompileError {
            diagnostics: vec![internal_error(&payload)],
            sources: SourceMap::default(),
            info: None,
        }),
    }
}

/// A diagnostic for a panic caught inside qlang itself.
pub(crate) fn internal_error(payload: &Box<dyn std::any::Any + Send>) -> Diagnostic {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".to_string());
    Diagnostic::error(
        "X999",
        format!("internal error: {detail} (this is a bug in qlang, please report it)"),
        Span::default(),
    )
}

/// The path given to the prelude's synthetic module, and the index it always loads at
/// (it is loaded before the entry file, and never imports anything itself).
const PRELUDE_PATH: &str = "<prelude>";
pub(crate) const PRELUDE_MOD: ModId = 0;

/// Declarations available in every file without an `import`: resolved like ordinary
/// qlang source (same checker, same diagnostics), just loaded as module 0 before the
/// entry file. A name declared locally always shadows one from here (`lookup_item`
/// checks the current module first).
const PRELUDE_SOURCE: &str = concat!(
    r#"
--" What a failed call gives back in a `Result`. --"
struct Error
    --" What went wrong, for a person to read. --"
    public message: string
end

--" The outcome of something that can fail: `Ok(value)` or `Err(error)`. Take it apart
with `match` (`case Ok(v) then ... case Err(e) then ...`), or with `is_ok()`, `unwrap()`...
Write `Ok(1)` / `Err(error)`, or `Result.Ok(1)` / `Result.Err(error)`. --"
enum Result<T, E>
    --" The success, with its value. --"
    Ok(value: T)
    --" The failure, with its error. --"
    Err(error: E)
end

impl<T, E> Result<T, E>
    --" Whether this is an ok result. --"
    public fun is_ok(self) -> bool
        match self
            case Ok(_) then true
            case Err(_) then false
        end
    end

    --" Whether this is an error result. --"
    public fun is_err(self) -> bool
        match self
            case Ok(_) then false
            case Err(_) then true
        end
    end

    --" The ok value, or stops the program if this is an error result. --"
    public fun unwrap(self) -> T
        match self
            case Ok(v) then v
            case Err(_) then panic("called unwrap() on an error Result")
        end
    end

    --" The error value, or stops the program if this is an ok result. --"
    public fun unwrap_err(self) -> E
        match self
            case Ok(_) then panic("called unwrap_err() on an ok Result")
            case Err(e) then e
        end
    end
end

--" An ok `Result`, usually inferred from context (`return Ok(1)` inside a function
returning `Result<int, Error>`, `let r: Result<int, Error> = Ok(1)`...). When the
compiler cannot infer `E` from anywhere, write `Result<int, Error>.Ok(1)` directly. --"
fun Ok<T, E>(value: T) -> Result<T, E>
    Result<T, E>.Ok(value)
end

--" An error `Result`: see `Ok`. --"
fun Err<T, E>(error: E) -> Result<T, E>
    Result<T, E>.Err(error)
end

-- ===== iteration =====

--" What one step of an iteration gives: `Item(value)`, or `End` when there is nothing left.
It is not a `T?`: an element may itself be `none`, and that must not stop the loop. --"
enum Step<T>
    --" The next element. --"
    Item(value: T)
    --" There is nothing left. --"
    End
end

--" Something that gives its elements one at a time: `next()` returns `Item(x)` until it
returns `End`. --"
trait Iterator<T>
    --" The next step of the iteration. --"
    fun next(self) -> Step<T>

    --" All the remaining elements, in an array. --"
    fun collect(self) -> array<T>
        let out: array<T> = []
        while true do
            match self.next()
                case Item(v) then out[] = v
                case End then return out
            end
        end
        out
    end

    --" How many elements remain (they are used up). --"
    fun count(self) -> int
        let n = 0
        while true do
            match self.next()
                case Item(_) then n += 1
                case End then return n
            end
        end
        n
    end

    --" The next element, or `none` when there is nothing left (an element that is itself
    `none` cannot be told apart: use `next()` for that). --"
    fun first(self) -> T?
        match self.next()
            case Item(v) then v
            case End then none
        end
    end

    --" An iterator over `f(x)` for each element `x`; nothing runs until it is used. --"
    fun map<U>(self, f: fun(T) -> U) -> Iterator<U>
        MapIter<T, U> { source: self, f: f }
    end

    --" An iterator over the elements for which `keep(x)` is true. --"
    fun filter(self, keep: fun(T) -> bool) -> Iterator<T>
        FilterIter<T> { source: self, keep: keep }
    end

    --" An iterator over the first `n` elements. --"
    fun take(self, n: int) -> Iterator<T>
        TakeIter<T> { source: self, left: n }
    end

    --" An iterator over the elements after the first `n`. --"
    fun skip(self, n: int) -> Iterator<T>
        SkipIter<T> { source: self, skip: n }
    end
end

--" The iterator of `map`: each element of `source` through `f`. --"
struct MapIter<T, U>
    --" The iterator it reads from. --"
    public source: Iterator<T>
    --" The function applied to each element. --"
    public f: fun(T) -> U
end

impl<T, U> Iterator<U> for MapIter<T, U>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<U>
        match self.source.next()
            case Item(v) then Step<U>.Item((self.f)(v))
            case End then Step<U>.End
        end
    end
end

--" The iterator of `filter`: the elements of `source` that `keep` accepts. --"
struct FilterIter<T>
    --" The iterator it reads from. --"
    public source: Iterator<T>
    --" Tells whether an element stays. --"
    public keep: fun(T) -> bool
end

impl<T> Iterator<T> for FilterIter<T>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<T>
        while true do
            match self.source.next()
                case Item(v) then
                    if (self.keep)(v) then
                        return Step<T>.Item(v)
                    end
                case End then
                    return Step<T>.End
            end
        end
        Step<T>.End
    end
end

--" The iterator of `take`: at most the first `left` elements of `source`. --"
struct TakeIter<T>
    --" The iterator it reads from. --"
    public source: Iterator<T>
    --" How many elements can still be given. --"
    public left: int
end

impl<T> Iterator<T> for TakeIter<T>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<T>
        if self.left <= 0 then
            return Step<T>.End
        end
        self.left -= 1
        self.source.next()
    end
end

--" The iterator of `skip`: `source` without its first elements. --"
struct SkipIter<T>
    --" The iterator it reads from. --"
    public source: Iterator<T>
    --" How many elements are still to be skipped. --"
    public skip: int
end

impl<T> Iterator<T> for SkipIter<T>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<T>
        while self.skip > 0 do
            self.skip -= 1
            match self.source.next()
                case Item(_) then
                case End then
                    return Step<T>.End
            end
        end
        self.source.next()
    end
end

--" A type that `for x in value`, `for i, x in value` and `[...value]` can go through. --"
trait Iterable<T>
    --" An iterator over the elements. --"
    fun iter(self) -> Iterator<T>
end

--" A type that `for key, value in x` can go through: its elements are pairs. --"
trait KeyedIterable<K, V>
    --" An iterator over the `(key, value)` pairs. --"
    fun iter_keyed(self) -> Iterator<(K, V)>
end

--" The iterator of an array, a string or a `Set`: its elements in order. --"
struct ArrayIter<T>
    --" The elements. --"
    public items: array<T>
    --" The position of the next one. --"
    public at: int
end

impl<T> Iterator<T> for ArrayIter<T>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<T>
        if self.at >= self.items.len() then
            return Step<T>.End
        end
        self.at += 1
        Step<T>.Item(self.items[self.at - 1])
    end
end

impl<T> Iterable<T> for array<T>
    --" An iterator over the elements. --"
    public fun iter(self) -> Iterator<T>
        ArrayIter<T> { items: self, at: 0 }
    end
end

impl Iterable<string> for string
    --" An iterator over the characters. --"
    public fun iter(self) -> Iterator<string>
        ArrayIter<string> { items: self.chars(), at: 0 }
    end
end

impl<T> Iterable<T> for Set<T>
    --" An iterator over the values, as they are now. --"
    public fun iter(self) -> Iterator<T>
        ArrayIter<T> { items: self.to_array(), at: 0 }
    end
end

--" The iterator of a map: its `(key, value)` pairs. --"
struct PairIter<K, V>
    --" The keys. --"
    public keys: array<K>
    --" The values, in the same order. --"
    public values: array<V>
    --" The position of the next pair. --"
    public at: int
end

impl<K, V> Iterator<(K, V)> for PairIter<K, V>
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<(K, V)>
        if self.at >= self.keys.len() then
            return Step<(K, V)>.End
        end
        self.at += 1
        Step<(K, V)>.Item((self.keys[self.at - 1], self.values[self.at - 1]))
    end
end

impl<K, V> KeyedIterable<K, V> for map<K, V>
    --" An iterator over the `(key, value)` pairs, as they are now. --"
    public fun iter_keyed(self) -> Iterator<(K, V)>
        PairIter<K, V> { keys: self.keys(), values: self.values(), at: 0 }
    end
end

-- ===== durations =====

--" A length of time: years and months (which depend on the date they are added to), then days, hours,
minutes, seconds and milliseconds (days are 24 hours: a `datetime` is always UTC). Nothing is converted on its
own: `Duration.of(minutes: 90)` stays 90 minutes until `normalize()`. Added to a `datetime` with `+`, `-`, `plus` or `minus`,
the years and months go first (31 January + 1 month is the last day of February). --"
struct Duration
    --" Whole years. --"
    public years: int
    --" Whole months. --"
    public months: int
    --" Days of 24 hours. --"
    public days: int
    --" Hours. --"
    public hours: int
    --" Minutes. --"
    public minutes: int
    --" Seconds. --"
    public seconds: int
    --" Milliseconds. --"
    public ms: int
end

impl Duration
    --" `Duration.of(days: 2, hours: 3)`: what is not given is 0. --"
    public fun of(years: int = 0, months: int = 0, days: int = 0, hours: int = 0, minutes: int = 0, seconds: int = 0, ms: int = 0) -> Duration
        Duration { years: years, months: months, days: days, hours: hours, minutes: minutes, seconds: seconds, ms: ms }
    end

    --" Reads the ISO 8601 text of a duration: `P1Y2M3DT4H5M6S`, `PT90M`, `P2W` (weeks are 7 days). `err` if it is not one. --"
    public fun parse(text: string) -> Result<Duration, Error>
        let bad = Result<Duration, Error>.Err(Error { message: "not an ISO 8601 duration: " + text })
        let cs = text.chars()
        if cs.len() < 2 or cs[0] != "P" then
            return bad
        end
        let d = Duration.of()
        let in_time = false
        let digits = ""
        let any = false
        for i in 1..cs.len() do
            let c = cs[i]
            if c == "T" then
                if in_time or digits != "" then
                    return bad
                end
                in_time = true
            else
                if c >= "0" and c <= "9" then
                    digits = digits + c
                else
                    let n = digits.to_int()
                    if n == none then
                        return bad
                    end
                    digits = ""
                    any = true
                    let ok = false
                    let v = n
                    if not in_time and c == "Y" then
                        d.years = v
                        ok = true
                    end
                    if not in_time and c == "M" then
                        d.months = v
                        ok = true
                    end
                    if not in_time and c == "W" then
                        d.days += v * 7
                        ok = true
                    end
                    if not in_time and c == "D" then
                        d.days += v
                        ok = true
                    end
                    if in_time and c == "H" then
                        d.hours = v
                        ok = true
                    end
                    if in_time and c == "M" then
                        d.minutes = v
                        ok = true
                    end
                    if in_time and c == "S" then
                        d.seconds = v
                        ok = true
                    end
                    if not ok then
                        return bad
                    end
                end
            end
        end
        if digits != "" or not any then
            return bad
        end
        Result<Duration, Error>.Ok(d)
    end

    --" Whether every part is 0. --"
    public fun is_zero(self) -> bool
        self.years == 0 and self.months == 0 and self.days == 0 and self.hours == 0 and self.minutes == 0 and self.seconds == 0 and self.ms == 0
    end

    --" Whether it has years or months: their length depends on the date, so there is no `total_*` of it. --"
    public fun has_calendar_parts(self) -> bool
        self.years != 0 or self.months != 0
    end

    --" The same length with the opposite sign. --"
    public fun negate(self) -> Duration
        Duration { years: -self.years, months: -self.months, days: -self.days, hours: -self.hours, minutes: -self.minutes, seconds: -self.seconds, ms: -self.ms }
    end

    --" Part by part sum. --"
    public fun plus(self, other: Duration) -> Duration
        Duration { years: self.years + other.years, months: self.months + other.months, days: self.days + other.days, hours: self.hours + other.hours, minutes: self.minutes + other.minutes, seconds: self.seconds + other.seconds, ms: self.ms + other.ms }
    end

    --" Part by part difference. --"
    public fun minus(self, other: Duration) -> Duration
        self.plus(other.negate())
    end

    --" Every part multiplied by `n`. --"
    public fun times(self, n: int) -> Duration
        Duration { years: self.years * n, months: self.months * n, days: self.days * n, hours: self.hours * n, minutes: self.minutes * n, seconds: self.seconds * n, ms: self.ms * n }
    end

    --" The length in milliseconds, or `err` if it has years or months. --"
    public fun total_ms(self) -> Result<int, Error>
        if self.has_calendar_parts() then
            return Result<int, Error>.Err(Error { message: "a duration with years or months has no fixed length: use datetime.until / diff_in_*" })
        end
        Result<int, Error>.Ok(((self.days * 24 + self.hours) * 60 + self.minutes) * 60000 + self.seconds * 1000 + self.ms)
    end

    --" The length in whole seconds (cut toward 0), or `err` if it has years or months. --"
    public fun total_seconds(self) -> Result<int, Error>
        let t = self.total_ms()
        if t.is_err() then
            return Result<int, Error>.Err(t.unwrap_err())
        end
        Result<int, Error>.Ok(t.unwrap() div 1000)
    end

    --" The length in whole minutes (cut toward 0), or `err` if it has years or months. --"
    public fun total_minutes(self) -> Result<int, Error>
        let t = self.total_ms()
        if t.is_err() then
            return Result<int, Error>.Err(t.unwrap_err())
        end
        Result<int, Error>.Ok(t.unwrap() div 60000)
    end

    --" The length in whole hours (cut toward 0), or `err` if it has years or months. --"
    public fun total_hours(self) -> Result<int, Error>
        let t = self.total_ms()
        if t.is_err() then
            return Result<int, Error>.Err(t.unwrap_err())
        end
        Result<int, Error>.Ok(t.unwrap() div 3600000)
    end

    --" The length in whole days (cut toward 0), or `err` if it has years or months. --"
    public fun total_days(self) -> Result<int, Error>
        let t = self.total_ms()
        if t.is_err() then
            return Result<int, Error>.Err(t.unwrap_err())
        end
        Result<int, Error>.Ok(t.unwrap() div 86400000)
    end

    --" The same length with the parts carried: 12 months are a year, 1000 ms a second, 60 seconds a minute,
    60 minutes an hour, 24 hours a day. The sign is that of the whole (every part gets it). --"
    public fun normalize(self) -> Duration
        let months = self.years * 12 + self.months
        let exact = ((self.days * 24 + self.hours) * 60 + self.minutes) * 60000 + self.seconds * 1000 + self.ms
        let m = months.abs()
        let e = exact.abs()
        let sm = if months < 0 then -1 else 1 end
        let se = if exact < 0 then -1 else 1 end
        Duration {
            years: sm * (m div 12),
            months: sm * (m % 12),
            days: se * (e div 86400000),
            hours: se * (e div 3600000 % 24),
            minutes: se * (e div 60000 % 60),
            seconds: se * (e div 1000 % 60),
            ms: se * (e % 1000)
        }
    end

    --" ISO 8601 text: `P1Y2M3DT4H5M6.007S` (`PT0S` when it is zero, a leading `-` when every part is negative). --"
    public fun to_iso(self) -> string
        if not self.is_zero() and self.years <= 0 and self.months <= 0 and self.days <= 0 and self.hours <= 0 and self.minutes <= 0 and self.seconds <= 0 and self.ms <= 0 then
            return "-" + self.negate().to_iso()
        end
        let date = ""
        if self.years != 0 then date += "{self.years}Y" end
        if self.months != 0 then date += "{self.months}M" end
        if self.days != 0 then date += "{self.days}D" end
        let time = ""
        if self.hours != 0 then time += "{self.hours}H" end
        if self.minutes != 0 then time += "{self.minutes}M" end
        if self.seconds != 0 or self.ms != 0 then
            let secs = "{self.seconds}"
            if self.ms != 0 then
                let frac = "{self.ms.abs()}".pad_left(3, "0")
                let sign = if self.seconds == 0 and self.ms < 0 then "-" else "" end
                secs = sign + "{self.seconds}" + "." + frac
            end
            time += secs + "S"
        end
        if date == "" and time == "" then
            return "PT0S"
        end
        if time == "" then "P" + date else "P" + date + "T" + time end
    end
end

--" The moments from a start to a limit, a `gap` apart: `datetime.of(2026, 1, 31).each(Duration.of(months: 1), until: datetime.of(2026, 5, 1))`.
The start is included, the limit too if it is reached. Each moment is the start plus `n` steps, so months do not drift
(31 January, 28 February, 31 March). --"
struct DatePeriod
    --" The first moment. --"
    public start: datetime
    --" The distance between two moments. --"
    public gap: Duration
    --" The limit, included when it is reached. --"
    public last: datetime
    --" How many moments were given. --"
    public count: int
end

impl DatePeriod
    --" An iterator over the same moments. --"
    public fun iter(self) -> Iterator<datetime>
        self
    end
end

impl Iterator<datetime> for DatePeriod
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<datetime>
        let t = self.start.plus(self.gap.times(self.count))
        if self.gap.is_zero() or t.is_after(self.last) then
            return Step<datetime>.End
        end
        self.count += 1
        Step<datetime>.Item(t)
    end
end

impl datetime
    --" The moments from here to `until`, one `gap` apart (the limit is included when it is reached). --"
    public fun each(self, gap: Duration, until: datetime) -> DatePeriod
        DatePeriod { start: self, gap: gap, last: until, count: 0 }
    end

    --" The instant after adding a duration: its years and months first, then the rest. --"
    public fun plus(self, d: Duration) -> datetime
        let t = self.add_years(d.years).add_months(d.months)
        let rest = ((d.days * 24 + d.hours) * 60 + d.minutes) * 60000 + d.seconds * 1000 + d.ms
        datetime.at(t.ms() + rest)
    end

    --" The instant after taking a duration away. --"
    public fun minus(self, d: Duration) -> datetime
        self.plus(d.negate())
    end

    --" Whether this instant is earlier than `other`. --"
    public fun is_before(self, other: datetime) -> bool
        self.ms() < other.ms()
    end

    --" Whether this instant is later than `other`. --"
    public fun is_after(self, other: datetime) -> bool
        self.ms() > other.ms()
    end

    --" Whether it is between `first` and `last`, both included. --"
    public fun between(self, first: datetime, last: datetime) -> bool
        self.ms() >= first.ms() and self.ms() <= last.ms()
    end

    --" Whether `other` is on the same day. --"
    public fun same_day(self, other: datetime) -> bool
        self.start_of_day().ms() == other.start_of_day().ms()
    end

    --" Saturday or Sunday. --"
    public fun is_weekend(self) -> bool
        self.weekday() >= 6
    end

    --" Whether the year has 366 days. --"
    public fun is_leap_year(self) -> bool
        let y = self.year()
        (y % 4 == 0 and y % 100 != 0) or y % 400 == 0
    end

    --" 28 to 31. --"
    public fun days_in_month(self) -> int
        let m = self.month()
        if m == 2 then
            return if self.is_leap_year() then 29 else 28 end
        end
        if m == 4 or m == 6 or m == 9 or m == 11 then 30 else 31 end
    end

    --" The quarter of the year, 1 to 4. --"
    public fun quarter(self) -> int
        (self.month() - 1) div 3 + 1
    end

    --" Whole months from this instant to `other` (negative if `other` is earlier): 31 January to 28 February is 1. --"
    public fun diff_in_months(self, other: datetime) -> int
        let n = (other.year() - self.year()) * 12 + (other.month() - self.month())
        if other.ms() >= self.ms() then
            if self.add_months(n).ms() > other.ms() then n -= 1 end
        else
            if self.add_months(n).ms() < other.ms() then n += 1 end
        end
        n
    end

    --" Whole years from this instant to `other`. --"
    public fun diff_in_years(self, other: datetime) -> int
        self.diff_in_months(other) div 12
    end

    --" Whole days (blocks of 24 hours) from this instant to `other`, cut toward 0. --"
    public fun diff_in_days(self, other: datetime) -> int
        (other.ms() - self.ms()) div 86400000
    end

    --" Whole hours from this instant to `other`, cut toward 0. --"
    public fun diff_in_hours(self, other: datetime) -> int
        (other.ms() - self.ms()) div 3600000
    end

    --" Whole minutes from this instant to `other`, cut toward 0. --"
    public fun diff_in_minutes(self, other: datetime) -> int
        (other.ms() - self.ms()) div 60000
    end

    --" Whole seconds from this instant to `other`, cut toward 0. --"
    public fun diff_in_seconds(self, other: datetime) -> int
        (other.ms() - self.ms()) div 1000
    end

    --" The time from this instant to `other` as a `Duration`: whole years and months first, then days, hours,
    minutes, seconds and milliseconds (every part has the sign of the direction). --"
    public fun until(self, other: datetime) -> Duration
        let months = self.diff_in_months(other)
        let rest = other.ms() - self.add_months(months).ms()
        Duration.of(months: months, ms: rest).normalize()
    end

    --" The start of the hour. --"
    public fun start_of_hour(self) -> datetime
        datetime.of(self.year(), self.month(), self.day(), self.hour(), 0, 0)
    end

    --" The last millisecond of the hour. --"
    public fun end_of_hour(self) -> datetime
        datetime.at(self.start_of_hour().ms() + 3599999)
    end

    --" The last millisecond of the day. --"
    public fun end_of_day(self) -> datetime
        datetime.at(self.start_of_day().ms() + 86399999)
    end

    --" Midnight of the Monday of this week. --"
    public fun start_of_week(self) -> datetime
        self.start_of_day().add_days(1 - self.weekday())
    end

    --" The last millisecond of the Sunday of this week. --"
    public fun end_of_week(self) -> datetime
        datetime.at(self.start_of_week().ms() + 7 * 86400000 - 1)
    end

    --" Midnight of the first day of the month. --"
    public fun start_of_month(self) -> datetime
        datetime.of(self.year(), self.month(), 1)
    end

    --" The last millisecond of the month. --"
    public fun end_of_month(self) -> datetime
        datetime.at(datetime.of(self.year(), self.month(), self.days_in_month()).ms() + 86399999)
    end

    --" Midnight of the first day of the quarter. --"
    public fun start_of_quarter(self) -> datetime
        datetime.of(self.year(), (self.quarter() - 1) * 3 + 1, 1)
    end

    --" The last millisecond of the quarter. --"
    public fun end_of_quarter(self) -> datetime
        let last = self.quarter() * 3
        let m = datetime.of(self.year(), last, 1)
        datetime.at(datetime.of(self.year(), last, m.days_in_month()).ms() + 86399999)
    end

    --" Midnight of 1 January. --"
    public fun start_of_year(self) -> datetime
        datetime.of(self.year(), 1, 1)
    end

    --" The last millisecond of the year. --"
    public fun end_of_year(self) -> datetime
        datetime.at(datetime.of(self.year(), 12, 31).ms() + 86399999)
    end

    --" The same instant with some parts replaced: `d.with(month: 3, day: 1)`. A day the new month does
    not have becomes its last day. --"
    public fun with(self, year: int? = none, month: int? = none, day: int? = none, hour: int? = none, minute: int? = none, second: int? = none) -> datetime
        let y = year ?? self.year()
        let mo = month ?? self.month()
        let first = datetime.of(y, mo, 1)
        let d = day ?? self.day()
        if d > first.days_in_month() and day == none then
            d = first.days_in_month()
        end
        datetime.of(y, mo, d, hour ?? self.hour(), minute ?? self.minute(), second ?? self.second()).plus(Duration.of(ms: self.ms() % 1000))
    end

    --" Midnight of the next day that is `weekday` (1 Monday to 7 Sunday), after today. --"
    public fun next_weekday(self, weekday: int) -> datetime
        let delta = (weekday - self.weekday() + 6) % 7 + 1
        self.start_of_day().add_days(delta)
    end

    --" Midnight of the last day that was `weekday`, before today. --"
    public fun previous_weekday(self, weekday: int) -> datetime
        let delta = (self.weekday() - weekday + 6) % 7 + 1
        self.start_of_day().add_days(-delta)
    end

    --" Midnight of the `n`th `weekday` of this month (`n` from 1), or `none` if the month has no such day
    (a fifth Monday). --"
    public fun nth_weekday_of_month(self, n: int, weekday: int) -> datetime?
        let first = self.start_of_month()
        let delta = (weekday - first.weekday() + 7) % 7
        let day = 1 + delta + (n - 1) * 7
        if n < 1 or day > self.days_in_month() then
            return none
        end
        datetime.of(self.year(), self.month(), day)
    end

    --" Midnight of the last day of the month. --"
    public fun last_day_of_month(self) -> datetime
        datetime.of(self.year(), self.month(), self.days_in_month())
    end

    --" Seconds since 1970 (cut toward the past). --"
    public fun unix(self) -> int
        let ms = self.ms()
        (ms - ((ms % 1000) + 1000) % 1000) div 1000
    end

    --" `Tue, 03 Nov 2026 14:30:00 GMT`: the form of the HTTP `Date` and `Last-Modified` headers. --"
    public fun to_http(self) -> string
        self.format("ddd, DD MMM YYYY HH:mm:ss") + " GMT"
    end

    --" `Tue, 03 Nov 2026 14:30:00 +0000` (RFC 2822). --"
    public fun to_rfc2822(self) -> string
        self.format("ddd, DD MMM YYYY HH:mm:ss") + " +0000"
    end

    --" ISO 8601 text, as `"{d}"` gives it. --"
    public fun to_iso(self) -> string
        "{self}"
    end

    --" The time to `now` in words, in `locale` (`"en"` or `"fr"`): `3 days ago`, `in 2 hours`. --"
    public fun humanize(self, now: datetime, locale: string = "en") -> string
        let diff = self.ms() - now.ms()
        let past = diff < 0
        let secs = diff.abs() div 1000
        let n = 0
        let unit = ""
        if secs < 45 then
            return if locale == "fr" then (if past then "il y a quelques secondes" else "dans quelques secondes" end) else (if past then "a few seconds ago" else "in a few seconds" end) end
        end
        if secs < 3600 then
            n = (secs + 30) div 60
            unit = "minute"
        elseif secs < 86400 then
            n = (secs + 1800) div 3600
            unit = "hour"
        elseif secs < 86400 * 30 then
            n = (secs + 43200) div 86400
            unit = "day"
        elseif secs < 86400 * 365 then
            n = (secs + 1296000) div 2592000
            unit = "month"
        else
            n = (secs + 15768000) div 31536000
            unit = "year"
        end
        if n == 0 then n = 1 end
        if locale == "fr" then
            let fr = if unit == "minute" then "minute" elseif unit == "hour" then "heure" elseif unit == "day" then "jour" elseif unit == "month" then "mois" else "an" end
            if n > 1 and unit != "month" then fr += "s" end
            return if past then "il y a {n} {fr}" else "dans {n} {fr}" end
        end
        let en = if n > 1 then unit + "s" else unit end
        if past then "{n} {en} ago" else "in {n} {en}" end
    end
end

impl Add<Duration> for datetime
    --" `datetime + duration`: see `plus`. --"
    public fun add(self, d: Duration) -> datetime
        self.plus(d)
    end
end

impl Sub<Duration> for datetime
    --" `datetime - duration`: see `minus`. --"
    public fun sub(self, d: Duration) -> datetime
        self.minus(d)
    end
end

-- ===== neutral elements =====

--" A type with a value that changes nothing when added: `0` for an `int`, `0.0` for a `float`.
Written `T.zero()` in a generic function (`fun total<T: Add + Zero>(...)`) or an `impl`. --"
trait Zero
    --" The neutral element of the addition. --"
    fun zero() -> Self
end

--" A type with a value that changes nothing when multiplied: `1` for an `int`, `1.0` for a
`float`. Written `T.one()`, like `Zero`. --"
trait One
    --" The neutral element of the multiplication. --"
    fun one() -> Self
end

impl Zero for int
    --" `0`. --"
    public fun zero() -> int
        0
    end
end

impl Zero for float
    --" `0.0`. --"
    public fun zero() -> float
        0.0
    end
end

impl One for int
    --" `1`. --"
    public fun one() -> int
        1
    end
end

impl One for float
    --" `1.0`. --"
    public fun one() -> float
        1.0
    end
end

-- ===== encode =====

--" A type that can be turned into JSON text: `impl ToJson for Post ... end`. --"
trait ToJson
    fun to_json(self) -> string
end

--" A JSON string literal of `s`, quoted and escaped. --"
fun json_escape(s: string) -> string
    let out = "\""
    for c in s.chars() do
        if c == "\"" then
            out += "\\\""
        elseif c == "\\" then
            out += "\\\\"
        elseif c == "\n" then
            out += "\\n"
        elseif c == "\t" then
            out += "\\t"
        elseif c == "\r" then
            out += "\\r"
        else
            out += c
        end
    end
    out + "\""
end

--" A JSON array of `items`, each serialized through `to_json()`. --"
fun json_array<T: ToJson>(items: array<T>) -> string
    let parts: array<string> = []
    for it in items do
        parts[] = it.to_json()
    end
    "[" + parts.join(",") + "]"
end

--" A JSON object of `fields`, each value serialized through `to_json()`. Used from a
struct's own `to_json`: `json_object({ "title": self.title, "views": self.views })`. --"
fun json_object(fields: map<string, ToJson>) -> string
    let parts: array<string> = []
    for k, v in fields do
        parts[] = json_escape(k) + ":" + v.to_json()
    end
    "\{" + parts.join(",") + "\}"
end

--" Merges two JSON objects (as produced by `json_object`) into one with the keys of
both. Lets a struct's `to_json` extend its parent's without repeating its fields:
`json_merge(super.to_json(), json_object({ "role": self.role }))`. --"
fun json_merge(a: string, b: string) -> string
    if a == "\{\}" then
        return b
    end
    if b == "\{\}" then
        return a
    end
    a.substring(0, a.len() - 1) + "," + b.substring(1, b.len())
end

impl ToJson for int
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for float
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for bool
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for Json
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        self.to_text()
    end
end

impl ToJson for string
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        json_escape(self)
    end
end

impl ToJson for uuid
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

impl ToJson for ulid
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

impl ToJson for datetime
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

--" So an `array<T>` of any `ToJson` type (including a nested struct or another
array) can be used directly as a `json_object` field, with no need to call
`json_array` by hand. --"
impl<T: ToJson> ToJson for array<T>
    public fun to_json(self) -> string
        json_array(self)
    end
end

-- ===== reflection =====

--" An annotation as reflection reads it: the `name` of its struct (`Route`) and its arguments
by field name, as JSON values (numbers are floats). --"
struct AnnotationInfo
    --" The name of the annotation, without `#[` and `]`. --"
    public name: string
    --" The arguments given to the annotation, by name. --"
    public args: map<string, Json>
end

impl AnnotationInfo
    --" The text argument `field`, or `none` (no such argument, or not a string). --"
    public fun get_string(self, field: string) -> string?
        let j = self.args[field]
        if j == none or not j.is_string() then none else j.as_string() end
    end

    --" The integer argument `field`, or `none`. --"
    public fun get_int(self, field: string) -> int?
        let j = self.args[field]
        if j == none or not j.is_number() then none else j.as_number() as int end
    end

    --" The number argument `field`, or `none`. --"
    public fun get_float(self, field: string) -> float?
        let j = self.args[field]
        if j == none or not j.is_number() then none else j.as_number() end
    end

    --" The bool argument `field`, or `none`. --"
    public fun get_bool(self, field: string) -> bool?
        let j = self.args[field]
        if j == none or not j.is_bool() then none else j.as_bool() end
    end
end

--" A parameter of a function or a variant, as reflection describes it. --"
struct ParamInfo
    --" The name of the parameter. --"
    public name: string
    --" `true` for `...name: type`, which takes the arguments left over. --"
    public variadic: bool
    --" The name of its type, as written (for a variadic parameter, the type of one element). --"
    public type_name: string
    --" The annotations of the parameter. --"
    public annotations: array<AnnotationInfo>
    ty: Any
end

impl ParamInfo
    --" Reads `json` as the type of this parameter (`Json.to<T>` when `T` is only known by
    reflection). The `Any` holds a value of that type. --"
    public fun decode(self, json: Json) -> Result<Any, Error>
        Reflect.decode(self.ty, json)
    end

    --" Reads `text` (a path segment, a query parameter) as the type of this parameter: `int`,
    `float`, `bool`, `string`, `uuid`, `ulid`, `datetime`, or one of them followed by `?` (empty
    text is then `none`). --"
    public fun parse(self, text: string) -> Result<Any, Error>
        Reflect.parse_text(self.ty, text)
    end

    --" The shape of the type of this parameter. --"
    public fun type_info(self) -> TypeInfo
        Reflect.type_info_of(self.ty)
    end
end

--" The shape of a type, as `Reflect.type_info<T>()` describes it: enough to write a schema. --"
struct TypeInfo
    --" `int`, `float`, `bool`, `string`, `uuid`, `ulid`, `datetime`, `bytes`, `any`, `unit`,
    `json`, `array`, `map`, `tuple`, `struct`, `enum`, `function` or `other`. A `T?` has the kind
    of `T` and `nullable` set. --"
    public kind: string
    --" The name of a struct or an enum, else the type as written. --"
    public name: string
    --" `true` for a nullable type (`T?`). --"
    public nullable: bool
    --" The element of an array, the value of a map. --"
    public item: TypeInfo?
    --" The fields `Json.of` writes: public, not `#[JsonSkip]`, under their `#[JsonName]`. --"
    public fields: array<FieldType>
    --" The variants of an enum. --"
    public variants: array<string>
    --" The type arguments of a generic struct or enum: `Result<Post, HttpError>` has two. --"
    public args: array<TypeInfo>
    --" Set on a struct met again inside itself: its fields are left out there. --"
    public recursive: bool
end

--" A field of a struct for a schema: its name in JSON, its shape and its annotations. --"
struct FieldType
    --" The name in JSON. --"
    public name: string
    --" The name of the field. --"
    public field: string
    --" The name of the type, as written. --"
    public type_name: string
    --" The annotations of the type. --"
    public annotations: array<AnnotationInfo>
    --" What `TypeInfo` says about the type. --"
    public info: TypeInfo
end

impl FieldType
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

--" A field of a struct, as reflection describes it. --"
struct FieldInfo
    --" The name of the field. --"
    public name: string
    --" The name of its type, as written. --"
    public type_name: string
    --" The annotations of the field. --"
    public annotations: array<AnnotationInfo>
end

--" A struct, as `Reflect.struct_info<T>()` describes it. --"
struct StructInfo
    --" The name of the structure. --"
    public name: string
    --" Its fields, in order. --"
    public fields: array<FieldInfo>
    --" Its annotations. --"
    public annotations: array<AnnotationInfo>
end

--" A variant of an enum, as reflection describes it. --"
struct VariantInfo
    --" The name of the variant. --"
    public name: string
    --" The data it carries, in order. --"
    public fields: array<ParamInfo>
    --" Its annotations. --"
    public annotations: array<AnnotationInfo>
end

--" An enum, as `Reflect.enum_info<T>()` describes it. --"
struct EnumInfo
    --" The name of the enumeration. --"
    public name: string
    --" Its variants, in order. --"
    public variants: array<VariantInfo>
    --" Its annotations. --"
    public annotations: array<AnnotationInfo>
end

--" A function, as `Reflect.functions()` describes it. --"
struct FunctionInfo
    --" The name of the function. --"
    public name: string
    --" `true` for a method (it takes `self`). --"
    public has_self: bool
    --" Its parameters, `self` excluded. --"
    public params: array<ParamInfo>
    --" The name of its return type, as written. --"
    public return_type: string
    --" Its annotations. --"
    public annotations: array<AnnotationInfo>
end

--" The annotation called `name` in `list`, or `none`. --"
fun reflect_find_annotation(list: array<AnnotationInfo>, name: string) -> AnnotationInfo?
    for a in list do
        if a.name == name then
            return a
        end
    end
    none
end

impl ParamInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl FieldInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl StructInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl VariantInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl EnumInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl FunctionInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

--" The inside of an enumeration value, as `Reflect.parts` gives it: the name of the enum, of its
variant and the data it carries (each as an `Any` with its own type). --"
struct EnumParts
    --" The name of the enumeration. --"
    public enum_name: string
    --" The name of the variant. --"
    public variant: string
    --" The data of the variant, in order. --"
    public values: array<Any>
end

--" A function found by `Reflect.callable(name)` (or `Reflect.callable(module, name)`), to call with
`Any` values: `call` checks their number and types against `params`. --"
struct Callable
    --" The name of the function. --"
    public name: string
    --" Its parameters. --"
    public params: array<ParamInfo>
    --" The name of its return type, as written. --"
    public return_type: string
    --" Its annotations. --"
    public annotations: array<AnnotationInfo>
    target: Any
    ret: Any
end

impl Callable
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end

    --" Calls the function. `err` when the number of arguments or one type is wrong. --"
    public fun call(self, args: array<Any>) -> Result<Any, Error>
        Reflect.invoke(self.target, args)
    end

    --" The shape of what the function returns. --"
    public fun return_info(self) -> TypeInfo
        Reflect.type_info_of(self.ret)
    end
end

--" Names a field differently in `Json.of` and `Json.to`: `#[JsonName("user_id")]`. --"
#[annotation]
struct JsonName
    name: string
end

--" Leaves a field out of `Json.of`. --"
#[annotation]
struct JsonSkip
end

--" Reflection: `Reflect.struct_info<T>()`, `Reflect.enum_info<T>()`, `Reflect.functions()` (the functions
of this file) and `Reflect.find<fun(A) -> B>("name")` (a function of this file, if its type is
that one); `Reflect.methods<T>()` and `Reflect.find_method<T, F>(value, "name")` do the same
for the methods of a type. --"
struct Reflect
end

-- ===== value tree (decode target) =====

--" A parsed JSON value: a string, a number, a bool, null, an array or an object.
Build one with `Json.string(...)`, `Json.number(...)`, `Json.array(...)`... (or parse
text with `Json.parse`), read it back with `is_string()`/`as_string()` and friends. --"
struct Json
    kind: string
    str: string?
    num: float?
    bool_: bool?
    arr: array<Json>?
    obj: map<string, Json>?
end

impl Json
    --" The JSON text of this value: `{"a":1,"b":[true,null]}`. --"
    public fun to_text(self) -> string
        Reflect.json_text(self)
    end

    --" The JSON `null`. --"
    public fun null() -> Json
        Json { kind: "null", str: none, num: none, bool_: none, arr: none, obj: none }
    end

    --" A JSON string. --"
    public fun string(value: string) -> Json
        Json { kind: "string", str: value, num: none, bool_: none, arr: none, obj: none }
    end

    --" A JSON number. --"
    public fun number(value: float) -> Json
        Json { kind: "number", str: none, num: value, bool_: none, arr: none, obj: none }
    end

    --" A JSON `true` or `false`. --"
    public fun boolean(value: bool) -> Json
        Json { kind: "bool", str: none, num: none, bool_: value, arr: none, obj: none }
    end

    --" A JSON array. --"
    public fun array(value: array<Json>) -> Json
        Json { kind: "array", str: none, num: none, bool_: none, arr: value, obj: none }
    end

    --" A JSON object. --"
    public fun object(value: map<string, Json>) -> Json
        Json { kind: "object", str: none, num: none, bool_: none, arr: none, obj: value }
    end

    --" `true` for the JSON `null`. --"
    public fun is_null(self) -> bool
        self.kind == "null"
    end

    --" `true` for a JSON string. --"
    public fun is_string(self) -> bool
        self.kind == "string"
    end

    --" `true` for a JSON number. --"
    public fun is_number(self) -> bool
        self.kind == "number"
    end

    --" `true` for a JSON `true` or `false`. --"
    public fun is_bool(self) -> bool
        self.kind == "bool"
    end

    --" `true` for a JSON array. --"
    public fun is_array(self) -> bool
        self.kind == "array"
    end

    --" `true` for a JSON object. --"
    public fun is_object(self) -> bool
        self.kind == "object"
    end

    --" The string; stops the program with an error if the value is not a string. --"
    public fun as_string(self) -> string
        let v = self.str
        if v != none then v else panic("Json: not a string") end
    end

    --" The number; stops the program with an error if the value is not a number. --"
    public fun as_number(self) -> float
        let v = self.num
        if v != none then v else panic("Json: not a number") end
    end

    --" The boolean; stops the program with an error if the value is not a boolean. --"
    public fun as_bool(self) -> bool
        let v = self.bool_
        if v != none then v else panic("Json: not a bool") end
    end

    --" The elements; stops the program with an error if the value is not an array. --"
    public fun as_array(self) -> array<Json>
        let v = self.arr
        if v != none then v else panic("Json: not an array") end
    end

    --" The members; stops the program with an error if the value is not an object. --"
    public fun as_object(self) -> map<string, Json>
        let v = self.obj
        if v != none then v else panic("Json: not an object") end
    end

    --" The member `key` of an object, or `none` when there is none (or the value is not an object). --"
    public fun get(self, key: string) -> Json?
        let o = self.obj
        if o != none then o[key] else none end
    end

    --" The element `i` of an array, or `none` when out of range (or the value is not an array). --"
    public fun at(self, i: int) -> Json?
        let a = self.arr
        if a != none and i >= 0 and i < a.len() then a[i] else none end
    end
end

impl ToJson for Json
    --" The JSON text of the value. --"
    public fun to_json(self) -> string
        if self.kind == "null" then
            "null"
        elseif self.kind == "string" then
            json_escape(self.as_string())
        elseif self.kind == "number" then
            self.as_number() as string
        elseif self.kind == "bool" then
            self.as_bool() as string
        elseif self.kind == "array" then
            let out: array<ToJson> = []
            for it in self.as_array() do
                out[] = it
            end
            json_array(out)
        else
            let fields: map<string, ToJson> = {}
            for k, v in self.as_object() do
                fields[k] = v
            end
            json_object(fields)
        end
    end
end

-- ===== parse =====

fun hex_digit_value(c: string) -> int?
    if c.is_digit() then
        c.to_int()
    elseif c == "a" or c == "A" then
        10
    elseif c == "b" or c == "B" then
        11
    elseif c == "c" or c == "C" then
        12
    elseif c == "d" or c == "D" then
        13
    elseif c == "e" or c == "E" then
        14
    elseif c == "f" or c == "F" then
        15
    else
        none
    end
end

fun parse_hex4(hex: string) -> int?
    if hex.len() != 4 then
        return none
    end
    let a = hex_digit_value(hex.substring(0, 1))
    let b = hex_digit_value(hex.substring(1, 2))
    let c = hex_digit_value(hex.substring(2, 3))
    let d = hex_digit_value(hex.substring(3, 4))
    if a == none or b == none or c == none or d == none then
        return none
    end
    a * 4096 + b * 256 + c * 16 + d
end

--" The reader behind `Json.parse` (internal). --"
struct JsonParser
    text: string
    pos: int
end

impl JsonParser
    --" A parser for `text`, at its start. Use `Json.parse` rather than this. --"
    public fun new(text: string) -> JsonParser
        JsonParser { text, pos: 0 }
    end

    --" The character at the current position (an empty string at the end). --"
    public fun peek(self) -> string
        if self.pos < self.text.len() then self.text.substring(self.pos, self.pos + 1) else "" end
    end

    --" Move one character on. --"
    public fun advance(self)
        self.pos = self.pos + 1
    end

    --" Skip the spaces, tabs and line breaks. --"
    public fun skip_ws(self)
        while self.peek() == " " or self.peek() == "\t" or self.peek() == "\n" or self.peek() == "\r" do
            self.advance()
        end
    end

    --" `true` when the text continues with `word`. --"
    public fun looking_at(self, word: string) -> bool
        let end_ = (self.pos + word.len()).min(self.text.len())
        self.text.substring(self.pos, end_) == word
    end

    --" An `err` that says `message` and where the parser is. --"
    public fun fail(self, message: string) -> Result<Json, Error>
        Result<Json, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: {message}" })
    end

    --" An error when something other than spaces follows the value, else `none`. --"
    public fun trailing_error(self) -> Error?
        self.skip_ws()
        if self.pos != self.text.len() then
            Error { message: "JSON parse error at offset {self.pos}: unexpected trailing characters" }
        else
            none
        end
    end

    --" Read the next JSON value. --"
    public fun parse_value(self) -> Result<Json, Error>
        self.skip_ws()
        let c = self.peek()
        if c == "\"" then
            self.parse_string_value()
        elseif c == "\{" then
            self.parse_object()
        elseif c == "[" then
            self.parse_array()
        elseif self.looking_at("true") then
            self.pos = self.pos + 4
            Result<Json, Error>.Ok(Json.boolean(true))
        elseif self.looking_at("false") then
            self.pos = self.pos + 5
            Result<Json, Error>.Ok(Json.boolean(false))
        elseif self.looking_at("null") then
            self.pos = self.pos + 4
            Result<Json, Error>.Ok(Json.null())
        elseif c == "-" or c.is_digit() then
            self.parse_number()
        else
            self.fail("unexpected character {json_escape(c)}")
        end
    end

    --" Read a string literal and return its content, escapes decoded. --"
    public fun parse_raw_string(self) -> Result<string, Error>
        self.advance()
        let out = ""
        while self.peek() != "\"" do
            if self.peek() == "" then
                return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: unterminated string" })
            end
            if self.peek() == "\\" then
                self.advance()
                let e = self.peek()
                if e == "\"" then
                    out += "\""
                elseif e == "\\" then
                    out += "\\"
                elseif e == "/" then
                    out += "/"
                elseif e == "n" then
                    out += "\n"
                elseif e == "t" then
                    out += "\t"
                elseif e == "r" then
                    out += "\r"
                elseif e == "b" then
                    out += (8).char()
                elseif e == "f" then
                    out += (12).char()
                elseif e == "u" then
                    let hex = self.text.substring(self.pos + 1, self.pos + 5)
                    let code = parse_hex4(hex)
                    if code == none then
                        return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: invalid \\u escape" })
                    end
                    out += code.char()
                    self.pos = self.pos + 4
                else
                    return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: invalid escape \\{e}" })
                end
                self.advance()
            else
                out += self.peek()
                self.advance()
            end
        end
        self.advance()
        Result<string, Error>.Ok(out)
    end

    --" Read a string literal as a JSON value. --"
    public fun parse_string_value(self) -> Result<Json, Error>
        let r = self.parse_raw_string()
        if r.is_err() then
            return Result<Json, Error>.Err(r.unwrap_err())
        end
        Result<Json, Error>.Ok(Json.string(r.unwrap()))
    end

    --" Read a number. --"
    public fun parse_number(self) -> Result<Json, Error>
        let start = self.pos
        if self.peek() == "-" then
            self.advance()
        end
        while self.peek().is_digit() do
            self.advance()
        end
        if self.peek() == "." then
            self.advance()
            while self.peek().is_digit() do
                self.advance()
            end
        end
        if self.peek() == "e" or self.peek() == "E" then
            self.advance()
            if self.peek() == "+" or self.peek() == "-" then
                self.advance()
            end
            while self.peek().is_digit() do
                self.advance()
            end
        end
        let text = self.text.substring(start, self.pos)
        let n = text.to_float()
        if n == none then
            return Result<Json, Error>.Err(Error { message: "JSON parse error at offset {start}: invalid number" })
        end
        Result<Json, Error>.Ok(Json.number(n))
    end

    --" Read an array. --"
    public fun parse_array(self) -> Result<Json, Error>
        self.advance()
        let items: array<Json> = []
        self.skip_ws()
        if self.peek() == "]" then
            self.advance()
            return Result<Json, Error>.Ok(Json.array(items))
        end
        while true do
            let r = self.parse_value()
            if r.is_err() then
                return Result<Json, Error>.Err(r.unwrap_err())
            end
            items[] = r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "]" then
                self.advance()
                return Result<Json, Error>.Ok(Json.array(items))
            else
                return self.fail("expected ',' or ']'")
            end
        end
        self.fail("unreachable")
    end

    --" Read an object. --"
    public fun parse_object(self) -> Result<Json, Error>
        self.advance()
        let fields: map<string, Json> = {}
        self.skip_ws()
        if self.peek() == "\}" then
            self.advance()
            return Result<Json, Error>.Ok(Json.object(fields))
        end
        while true do
            self.skip_ws()
            if self.peek() != "\"" then
                return self.fail("expected a string key")
            end
            let key_r = self.parse_raw_string()
            if key_r.is_err() then
                return Result<Json, Error>.Err(key_r.unwrap_err())
            end
            self.skip_ws()
            if self.peek() != ":" then
                return self.fail("expected ':'")
            end
            self.advance()
            let value_r = self.parse_value()
            if value_r.is_err() then
                return Result<Json, Error>.Err(value_r.unwrap_err())
            end
            fields[key_r.unwrap()] = value_r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "\}" then
                self.advance()
                return Result<Json, Error>.Ok(Json.object(fields))
            else
                return self.fail("expected ',' or '\}'")
            end
        end
        self.fail("unreachable")
    end
end

impl Json
    --" Parses JSON text into a `Json` value, or an `Error` describing what went wrong. --"
    public fun parse(text: string) -> Result<Json, Error>
        let p = JsonParser.new(text)
        let r = p.parse_value()
        if r.is_err() then
            return r
        end
        let trailing = p.trailing_error()
        if trailing != none then
            return Result<Json, Error>.Err(trailing)
        end
        r
    end
end

--" An HTTP request: the one given to `http.send`, and the one a program run with
`qlang serve` receives. For a received request, `url` is the target as received
(`/items?id=3`), `path` its decoded path (`/items`) and `query` the decoded parameters
(`id` is `"3"`; the last one wins when a name is repeated); `headers` has lower-case names.
`params` holds the parameters of the route a `Router` matched (`/items/:id`). --"
struct Request
    --" The HTTP method, in capitals (`GET`). --"
    public method: string
    --" The target (`/items?id=3`) for a received request, the address to call for `http.send`. --"
    public url: string
    --" The headers; for a received request the names are in lower case. --"
    public headers: map<string, string> = {}
    --" The body as text, or `none` when there is none or it is not text. --"
    public body: string? = none
    --" The body as bytes, for a body that is not text (an upload, an image). For a received
    request it is the exact body; for `http.send` it wins over `body`. --"
    public raw: bytes? = none
    --" The decoded path of a received request (`/items`). --"
    public path: string = ""
    --" The decoded query parameters (the last one wins when a name is repeated). --"
    public query: map<string, string> = {}
    --" The parameters of the route a `Router` matched (`/items/:id`). --"
    public params: map<string, string> = {}
    --" Every value of each query parameter, in order (`?a=1&a=2` gives `{"a": ["1", "2"]}`). --"
    public query_all: map<string, array<string>> = {}
    --" Every value of each header (lower-case names): a header sent twice has two values. --"
    public headers_all: map<string, array<string>> = {}
    --" The cookies of the `Cookie` header, with their values decoded. --"
    public cookies: map<string, string> = {}
    --" What the program attaches to this request while it handles it (the user an
    authentication step found, for example); read back with `as<T>()`. --"
    public attrs: map<string, Any> = {}
end

--" One part of a `multipart/form-data` body: a field of the form or a file. --"
struct Part
    --" The name of the form field. --"
    public name: string
    --" The name of the uploaded file, or `none` for an ordinary field. --"
    public filename: string? = none
    --" The type of the content (`image/png`), when the client said it. --"
    public content_type: string? = none
    --" The content, as bytes. --"
    public data: bytes
end

impl Part
    --" The content as text, or `none` if it is not valid text. --"
    public fun text(self) -> string?
        self.data.to_string()
    end
end

fun multipart_param(header: string, key: string) -> string?
    let found = regex.captures(header, "(?i)(?:^|[;\\s])" + key + "=\"([^\"]*)\"")
    if found == none then
        return none
    end
    found[1]
end

fun multipart_boundary(content_type: string) -> string?
    let found = regex.captures(content_type, "(?i)boundary=(?:\"([^\"]+)\"|([^;\\s]+))")
    if found == none then
        return none
    end
    let quoted = found[1]
    if quoted != none then
        return quoted
    end
    found[2]
end

fun multipart_error(message: string) -> Result<array<Part>, Error>
    Result<array<Part>, Error>.Err(Error { message: message })
end

impl Request
    --" The parts of a `multipart/form-data` body (an HTML form with files): `err` when the request
    is not one or its body is malformed. The files are in `Part.data`. --"
    public fun multipart(self) -> Result<array<Part>, Error>
        let kind = self.header("content-type")
        if kind == none or not kind.lower().starts_with("multipart/form-data") then
            return multipart_error("the body is not multipart/form-data")
        end
        let boundary = multipart_boundary(kind)
        let raw = self.raw
        if boundary == none or raw == none then
            return multipart_error("the multipart body has no boundary or no content")
        end
        let delim = "--{boundary}".to_bytes()
        let crlf = "\r\n".to_bytes()
        let blank = "\r\n\r\n".to_bytes()
        let after = "\r\n--{boundary}".to_bytes()
        let first = raw.find(delim)
        if first == none then
            return multipart_error("the multipart body does not start with its boundary")
        end
        let parts: array<Part> = []
        let pos = first + delim.len()
        let total = raw.len()
        while true do
            let rest = raw.slice(pos, total)
            if rest.starts_with("--".to_bytes()) then
                break
            end
            if not rest.starts_with(crlf) then
                return multipart_error("a multipart boundary is not followed by a line break")
            end
            let body = rest.slice(2, rest.len())
            let head_end = body.find(blank)
            if head_end == none then
                return multipart_error("a multipart part has no end of headers")
            end
            let head_text = body.slice(0, head_end).to_string()
            if head_text == none then
                return multipart_error("the headers of a multipart part are not text")
            end
            let content = body.slice(head_end + 4, body.len())
            let stop = content.find(after)
            if stop == none then
                return multipart_error("a multipart part is not closed by a boundary")
            end
            let disposition = ""
            let content_type: string? = none
            for line in head_text.split("\r\n") do
                let lowered = line.lower()
                if lowered.starts_with("content-disposition:") then
                    disposition = line
                end
                if lowered.starts_with("content-type:") then
                    content_type = line.substring(13, line.len()).trim()
                end
            end
            let name = multipart_param(disposition, "name")
            if name == none then
                return multipart_error("a multipart part has no name")
            end
            parts.push(Part { name: name, filename: multipart_param(disposition, "filename"), content_type: content_type, data: content.slice(0, stop) })
            pos = total - content.len() + stop + 2 + delim.len()
        end
        Result<array<Part>, Error>.Ok(parts)
    end

    --" The values of the header `name` (whatever the case of `name`); empty when there is none. --"
    public fun header_all(self, name: string) -> array<string>
        let found = self.headers_all[name.lower()]
        if found == none then [] else found end
    end

    --" The fields of a body sent as `application/x-www-form-urlencoded`, decoded. `err` when
    the request is not a form. --"
    public fun form(self) -> Result<map<string, string>, Error>
        let kind = self.header("content-type")
        if kind == none or not kind.lower().starts_with("application/x-www-form-urlencoded") then
            return Result<map<string, string>, Error>.Err(Error { message: "the body is not a form (application/x-www-form-urlencoded)" })
        end
        let out: map<string, string> = {}
        let body = self.body
        if body == none then
            return Result<map<string, string>, Error>.Ok(out)
        end
        for pair in body.split("&") do
            if pair == "" then
                continue
            end
            let parts = pair.split("=")
            let key = Url.decode(parts[0])
            let value = Url.decode(parts.slice(1, parts.len()).join("="))
            if key == none or value == none then
                return Result<map<string, string>, Error>.Err(Error { message: "the form has a badly encoded field" })
            end
            out[key] = value
        end
        Result<map<string, string>, Error>.Ok(out)
    end

    --" The value of the header `name` of a received request (whatever the case of `name`),
    or `none`. --"
    public fun header(self, name: string) -> string?
        self.headers[name.lower()]
    end
end

--" An HTTP response: the one returned by `http.send`, and the one the `handle` function of
a program run with `qlang serve` returns. --"
struct Response
    --" The HTTP status (`200`, `404`…). --"
    public status: int
    --" The headers. --"
    public headers: map<string, string> = {}
    --" The body. --"
    public body: string
    --" The exact bytes of the body when it is not text; `body` is then a lossy text. A
    program that answers with `raw` set sends these bytes instead of `body`. --"
    public raw: bytes? = none
    --" The cookies to set: one `Set-Cookie` header each. --"
    public cookies: array<Cookie> = []
    --" A body produced piece by piece: the server calls this function again and again and sends
    each text it gives, until it gives `none`. The worker that answers is busy until then. --"
    public stream: (fun() -> string?)? = none
    --" A file of the server's disk sent as the body (see `Response.file`): the server reads it piece
    by piece, so a big file does not fill the memory and the worker is free at once. --"
    public file: string? = none
    --" Turns the connection into a WebSocket (see `Response.websocket`). --"
    public websocket: WebSocketHandler? = none
end

--" A message received on a WebSocket: `text` for a text message, `data` for a binary one. --"
struct WsMessage
    --" The text of a text message, or `none` for a binary one. --"
    public text: string? = none
    --" The bytes of a binary message, or `none` for a text one. --"
    public data: bytes? = none
    --" `true` for a binary message. --"
    public binary: bool = false
end

--" One thing a WebSocket has to send; `WebSocket.send` and friends make them. --"
struct WsFrame
    --" `text`, `binary` or `close`. --"
    public kind: string
    --" The text of a text frame. --"
    public text: string = ""
    --" The bytes of a binary frame. --"
    public data: bytes = bytes.empty()
    --" The code of a close frame (1000: normal). --"
    public code: int = 1000
end

--" One WebSocket connection, as the callbacks of a `WebSocketHandler` see it. Many connections
live in the same worker: do not wait inside a callback. --"
struct WebSocket
    --" A unique identifier of this connection. --"
    public id: string
    --" The request that opened it (path, query, cookies, what a guard put in `attrs`). --"
    public request: Request
    --" Whatever you want to remember about this connection (who it is, the room it joined). --"
    public data: map<string, Any> = {}
    outbox: array<WsFrame> = []
    open: bool = true
end

impl WebSocket
    --" Sends a text message. --"
    public fun send(self, text: string)
        if self.open then
            self.outbox.push(WsFrame { kind: "text", text: text })
        end
    end

    --" Sends a binary message. --"
    public fun send_bytes(self, data: bytes)
        if self.open then
            self.outbox.push(WsFrame { kind: "binary", data: data })
        end
    end

    --" Closes the connection (code 1000: normal). --"
    public fun close(self)
        self.close_with(1000, "")
    end

    --" Closes the connection with a code (1000-4999) and a short reason. --"
    public fun close_with(self, code: int, reason: string)
        if self.open then
            self.open = false
            self.outbox.push(WsFrame { kind: "close", text: reason, code: code })
        end
    end

    --" `true` until the connection is closed. --"
    public fun is_open(self) -> bool
        self.open
    end

    --" What was queued to send since the last call, and forgets it: how a test reads what a
    callback wrote (the server does the same before sending). --"
    public fun take_frames(self) -> array<WsFrame>
        let out = self.outbox
        self.outbox = []
        out
    end
end

--" What happens on a WebSocket: each function is called with the connection, one event at a time
(a worker serves many connections, so a callback must be short). --"
struct WebSocketHandler
    --" A message arrived. --"
    public on_message: fun(WebSocket, WsMessage)
    --" The connection was opened. --"
    public on_open: (fun(WebSocket))? = none
    --" Called every `tick_ms` while the connection is open: poll a database, send the news. --"
    public on_tick: (fun(WebSocket))? = none
    --" How often, in milliseconds, `on_tick` is called (1000 by default; 0: never). --"
    public tick_ms: int = 1000
    --" The connection ended (the client left, or `close`). Nothing can be sent any more. --"
    public on_close: (fun(WebSocket))? = none
end

--" One event of a server-sent events stream (`Response.sse`). --"
struct SseEvent
    --" The data of the event (several lines make several `data:` lines). --"
    public data: string
    --" The name of the event (`event:`), or `none`. --"
    public event: string? = none
    --" The identifier of the event (`id:`), or `none`. --"
    public id: string? = none
    --" How long, in milliseconds, the client waits before reconnecting (`retry:`), or `none`. --"
    public retry: int? = none
end

impl SseEvent
    --" The event as the browser reads it: `event:`, `id:`, `retry:` and a `data:` line for each
    line of `data`, then an empty line. --"
    public fun to_text(self) -> string
        let out = ""
        if self.event != none then
            out += "event: {self.event}\n"
        end
        if self.id != none then
            out += "id: {self.id}\n"
        end
        if self.retry != none then
            out += "retry: {self.retry}\n"
        end
        for line in self.data.split("\n") do
            out += "data: {line}\n"
        end
        out + "\n"
    end
end

--" A cookie to send with a `Response`. `value` is sent percent-encoded (`Request.cookies` decodes
it); `max_age` is in seconds (`none`: a cookie for the session); `same_site` is `"Lax"`,
`"Strict"` or `"None"`. --"
struct Cookie
    --" The name of the cookie. --"
    public name: string
    --" Its value. --"
    public value: string
    --" The path it applies to. --"
    public path: string = "/"
    --" The domain it applies to, or `none` for the server's own. --"
    public domain: string? = none
    --" Its life in seconds, or `none` for a cookie that ends with the browser session. --"
    public max_age: int? = none
    --" `true`: only sent over HTTPS. --"
    public secure: bool = false
    --" `true`: not readable from the page's scripts. --"
    public http_only: bool = true
    --" `Lax`, `Strict` or `None`. --"
    public same_site: string = "Lax"
end

--" An error that has an HTTP status: what a handler gives back when it wants more than a 500. --"
struct HttpError
    --" The HTTP status of the answer. --"
    public status: int
    --" The message of the answer. --"
    public message: string
end

impl HttpError
    --" The response for this error: its message, as text, with its status. --"
    public fun to_response(self) -> Response
        Response { status: self.status, headers: { "Content-Type": "text/plain; charset=utf-8" }, body: self.message }
    end
end

impl Response
    --" A plain text response (status 200). --"
    public fun text(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "text/plain; charset=utf-8" }, body }
    end

    --" An HTML response (status 200). --"
    public fun html(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "text/html; charset=utf-8" }, body }
    end

    --" A JSON response (status 200) with the JSON text `body`. --"
    public fun json(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "application/json; charset=utf-8" }, body }
    end

    --" A response whose body is produced piece by piece: the server calls `next` again and again,
    sends each text it gives (right away) and ends when it gives `none`. For long answers and
    events. While it runs, the worker that answers serves only this connection (give
    `--workers` enough). --"
    public fun stream(content_type: string, next: fun() -> string?) -> Response
        Response { status: 200, headers: { "Content-Type": content_type, "Cache-Control": "no-cache", "X-Accel-Buffering": "no" }, body: "", stream: next }
    end

    --" Sends the file at `path` (`Content-Length` from its size, `Content-Type` from its extension,
    `application/octet-stream` when unknown): read piece by piece by the server, so it can be big. It is
    `404` when the file does not exist, and refused outside `--root`. The program decides who may get
    it: check the user first, and never build `path` from what the client wrote without cleaning it
    (`Path.normalize`). Change `headers["Content-Type"]` for another type. --"
    public fun file(path: string) -> Response
        Response { status: 200, headers: {}, body: "", file: path }
    end

    --" Like `file`, and the browser saves it under `filename` (`Content-Disposition: attachment`). --"
    public fun download(path: string, filename: string) -> Response
        let clean = filename.chars().filter(fun(c: string) -> bool c != "\"" and c != "\\" and c != ";" and c != "\n" and c != "\r" end).join("")
        Response { status: 200, headers: { "Content-Disposition": "attachment; filename=\"{clean}\"; filename*=UTF-8''{Url.encode(filename)}" }, body: "", file: path }
    end

    --" A server-sent events response (`text/event-stream`): `next` gives the events one after the
    other, waiting (`time.sleep`) between them if it wants; `none` ends the stream. --"
    public fun sse(next: fun() -> SseEvent?) -> Response
        Response.stream("text/event-stream", fun() -> string?
            let e = next()
            if e == none then none else e.to_text() end
        end)
    end

    --" Turns the request into a WebSocket (the request must ask for it: `Upgrade: websocket`).
    From then on `handler` is called for each event of the connection. --"
    public fun websocket(handler: WebSocketHandler) -> Response
        Response { status: 101, body: "", websocket: handler }
    end

    --" A response (status 200) with these bytes as its body and this `Content-Type`. --"
    public fun bytes(data: bytes, content_type: string) -> Response
        Response { status: 200, headers: { "Content-Type": content_type }, body: "", raw: data }
    end

    --" A redirection (status 302) to `location`. --"
    public fun redirect(location: string) -> Response
        Response { status: 302, headers: { "Location": location }, body: "" }
    end

    --" The 404 response. --"
    public fun not_found() -> Response
        Response { status: 404, headers: { "Content-Type": "text/plain; charset=utf-8" }, body: "Not Found" }
    end

    --" This response with another status (returns the response itself). --"
    public fun with_status(self, code: int) -> Response
        self.status = code
        self
    end

    --" This response with a cookie more (returns the response itself). --"
    public fun with_cookie(self, cookie: Cookie) -> Response
        self.cookies.push(cookie)
        self
    end

    --" This response with one more header (returns the response itself). --"
    public fun with_header(self, name: string, value: string) -> Response
        self.headers[name] = value
        self
    end
end

--" One route of a `Router`: the requests with this `method` (`"GET"`...) and a path matching
`pattern` go to `handler`. --"
struct Route
    --" The method the route answers (`GET`). --"
    public method: string
    --" The pattern of the path (`/items/:id`). --"
    public pattern: string
    --" The function that answers. --"
    public handler: fun(Request) -> Response
end

--" Sends each request to the route that matches it, so that a `handle` function does not need a
`match` on the path:

```
let router = Router.new()
router.get("/", fun(req: Request) -> Response Response.text("home") end)
router.get("/items/:id", fun(req: Request) -> Response
    Response.text("item " + req.params.get("id", ""))
end)

fun handle(req: Request) -> Response
    router.handle(req)
end
```

A pattern is made of segments between `/`: a word must match exactly, `:name` matches any
non-empty segment and puts it in `req.params`, and a final `*` matches the rest of the path
(`req.params["*"]`). The first route that matches wins. A path that matches a route with
another method gets a 405 response (with an `Allow` header), any other path a 404 --"
struct Router
    routes: array<Route>
    other: (fun(Request) -> Response)?
end

impl Router
    --" A router with no route. --"
    public fun new() -> Router
        Router { routes: [], other: none }
    end

    --" Adds a route for the requests with this `method` (`"GET"`, `"POST"`...). Returns the
    router, so calls can be chained. --"
    public fun add(self, method: string, pattern: string, handler: fun(Request) -> Response) -> Router
        self.routes[] = Route { method, pattern, handler }
        self
    end

    --" Adds a `GET` route. --"
    public fun get(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("GET", pattern, handler)
    end

    --" Adds a `POST` route. --"
    public fun post(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("POST", pattern, handler)
    end

    --" Adds a `PUT` route. --"
    public fun put(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("PUT", pattern, handler)
    end

    --" Adds a `DELETE` route. --"
    public fun delete(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("DELETE", pattern, handler)
    end

    --" What to answer when no route matches (instead of the 404 response). --"
    public fun fallback(self, handler: fun(Request) -> Response) -> Router
        self.other = handler
        self
    end

    --" The parameters of `path` if it matches `pattern`, or `none`. `/items/:id` matches
    `/items/3` (`id` is `"3"`); a final `*` takes the rest. --"
    public fun matches(pattern: string, path: string) -> map<string, string>?
        let wanted = pattern.split("/")
        let parts = path.split("/")
        let params: map<string, string> = {}
        let i = 0
        while i < wanted.len() do
            let w = wanted[i]
            if w == "*" and i == wanted.len() - 1 then
                if parts.len() <= i then
                    return none
                end
                params["*"] = parts.slice(i, parts.len()).join("/")
                return params
            end
            if i >= parts.len() then
                return none
            end
            if w.starts_with(":") then
                if parts[i] == "" then
                    return none
                end
                params[w.substring(1, w.len())] = parts[i]
            elseif w != parts[i] then
                return none
            end
            i += 1
        end
        if parts.len() != wanted.len() then none else params end
    end

    --" Answers `req` with the first route that matches it. --"
    public fun handle(self, req: Request) -> Response
        let allowed: array<string> = []
        for route in self.routes do
            let found = Router.matches(route.pattern, req.path)
            if found != none then
                if route.method == req.method then
                    req.params = found
                    return route.handler(req)
                end
                if not allowed.contains(route.method) then
                    allowed[] = route.method
                end
            end
        end
        let other = self.other
        if other != none then
            return other(req)
        end
        if allowed.len() > 0 then
            return Response { status: 405, headers: { "Allow": allowed.join(", "), "Content-Type": "text/plain; charset=utf-8" }, body: "Method Not Allowed" }
        end
        Response.not_found()
    end
end

-- The host provides the database: these are the `extern fun` it implements.
extern fun sql_connect(url: string) -> Result<int, Error>
extern fun sql_query(conn: int, sql: string, params: array<Json>) -> Result<array<map<string, Json>>, Error>
extern fun sql_execute(conn: int, sql: string, params: array<Json>) -> Result<int, Error>
extern fun sql_close(conn: int) -> Result<bool, Error>

--" Statement outcome as a `bool`: ok(true), or the same error. --"
fun sql_done(r: Result<int, Error>) -> Result<bool, Error>
    if r.is_ok() then
        Result.Ok(true)
    else
        Result.Err(r.unwrap_err())
    end
end

-- The host provides the disk, the process environment and the network too.
extern fun fs_read(path: string) -> Result<string, Error>
extern fun fs_write(path: string, content: string) -> Result<bool, Error>
extern fun fs_read_bytes(path: string) -> Result<bytes, Error>
extern fun fs_write_bytes(path: string, content: bytes) -> Result<bool, Error>
extern fun fs_create_dir(path: string) -> Result<bool, Error>
extern fun fs_exists(path: string) -> Result<bool, Error>
extern fun fs_is_dir(path: string) -> Result<bool, Error>
extern fun fs_list(path: string) -> Result<array<string>, Error>
extern fun fs_stat(path: string) -> Result<array<int>?, Error>
extern fun fs_append(path: string, content: string) -> Result<bool, Error>
extern fun fs_remove(path: string, recursive: bool) -> Result<bool, Error>
extern fun fs_rename(source: string, target: string) -> Result<bool, Error>
extern fun fs_copy(source: string, target: string) -> Result<bool, Error>
extern fun fs_open_lines(path: string) -> Result<int, Error>
extern fun fs_read_line(handle: int) -> Result<string?, Error>
extern fun fs_close_lines(handle: int) -> Result<bool, Error>
extern fun env_get(name: string) -> string?
extern fun env_args() -> array<string>
extern fun http_send(req: Request) -> Result<Response, Error>
extern fun http_send_all(reqs: array<Request>) -> Result<array<Response>, Error>

--" Files on disk: `fs.read(path)` and `fs.write(path, content)`. Needs disk access. --"
struct fs
end

impl fs
    --" Reads a file's whole content as text. Needs disk access. A missing file or a permission error is `err`, not a crash. --"
    public fun read(path: string) -> Result<string, Error>
        fs_read(path)
    end

    --" Writes `content` to a file, replacing it entirely (creating it if needed). Needs disk access. --"
    public fun write(path: string, content: string) -> Result<bool, Error>
        fs_write(path, content)
    end

    --" Reads a file's whole content as bytes (an image, an archive). Needs disk access. --"
    public fun read_bytes(path: string) -> Result<bytes, Error>
        fs_read_bytes(path)
    end

    --" Writes bytes to a file, replacing it entirely (creating it if needed). Needs disk access. --"
    public fun write_bytes(path: string, content: bytes) -> Result<bool, Error>
        fs_write_bytes(path, content)
    end

    --" Creates a folder, and the folders above it that are missing; nothing happens if it is already
    there. Needs disk access (and stays inside `--root`). --"
    public fun create_dir(path: string) -> Result<bool, Error>
        fs_create_dir(path)
    end

    --" Whether a file or a folder is there. Needs disk access. --"
    public fun exists(path: string) -> Result<bool, Error>
        fs_exists(path)
    end

    --" Whether `path` is a folder (`false` when it is not there). Needs disk access. --"
    public fun is_dir(path: string) -> Result<bool, Error>
        fs_is_dir(path)
    end

    --" The names (not the paths) of what a folder holds, sorted. Needs disk access. --"
    public fun list(path: string) -> Result<array<string>, Error>
        fs_list(path)
    end

    --" Whether `path` is a file (`false` when it is not there, or is a folder). Needs disk access. --"
    public fun is_file(path: string) -> Result<bool, Error>
        let st = fs_stat(path)
        if st.is_err() then
            return Result<bool, Error>.Err(st.unwrap_err())
        end
        let found = st.unwrap()
        Result<bool, Error>.Ok(found != none and found[0] % 2 == 1)
    end

    --" Adds `content` at the end of a file (creating it if needed). Needs disk access. --"
    public fun append(path: string, content: string) -> Result<bool, Error>
        fs_append(path, content)
    end

    --" Deletes a file (or a link). A folder is `remove_dir`. Needs disk access. --"
    public fun remove(path: string) -> Result<bool, Error>
        if kind_at(path) % 4 >= 2 then
            return Result<bool, Error>.Err(Error { message: "\"" + path + "\" is a folder: use fs.remove_dir" })
        end
        fs_remove(path, false)
    end

    --" Deletes a folder: only an empty one, unless `all` is true (then everything it holds goes too,
    be sure of the path). The `--root` folder itself and the one the program runs in are never removed.
    Needs disk access. --"
    public fun remove_dir(path: string, all: bool = false) -> Result<bool, Error>
        fs_remove(path, all)
    end

    --" Moves or renames a file or a folder (the target's folder must be there). Needs disk access. --"
    public fun rename(source: string, target: string) -> Result<bool, Error>
        fs_rename(source, target)
    end

    --" Copies a file to `target` (replacing what is there). Needs disk access. --"
    public fun copy(source: string, target: string) -> Result<bool, Error>
        fs_copy(source, target)
    end

    --" Opens a text file to read it one line at a time, without loading it all: `next()` gives each
    line (without its line break) until `none`, `close()` lets go early. A line may be 16 MB at most, and 64
    files can be open at once. Needs disk access. --"
    public fun lines(path: string) -> Result<Lines, Error>
        open_lines(path)
    end

    --" Calls `f` with each line of a text file, one at a time (the file is never all in memory), and
    gives how many there were. Needs disk access. --"
    public fun each_line(path: string, f: fun(string)) -> Result<int, Error>
        for_each_line(path, f)
    end

    --" The lines of a text file as an array (the whole file is loaded: for a big one, `lines` or
    `each_line`). Needs disk access. --"
    public fun read_lines(path: string) -> Result<array<string>, Error>
        let out: array<string> = []
        let r = for_each_line(path, fun(line: string) out.push(line) end)
        if r.is_err() then
            return Result<array<string>, Error>.Err(r.unwrap_err())
        end
        Result<array<string>, Error>.Ok(out)
    end

    --" The paths that match a pattern, sorted: `*` is any characters but `/`, `**` any characters
    including `/` (`src/**/*.q` goes into every folder; `**/` also matches no folder at all), `?` one
    character, `[a-c]` / `[!a-c]` a set. The pattern starts at the folder before its first wildcard.
    Needs disk access. --"
    public fun glob(pattern: string) -> Result<array<string>, Error>
        let parts = pattern.split("/")
        let fixed: array<string> = []
        for part in parts do
            if part.contains("*") or part.contains("?") or part.contains("[") then
                break
            end
            fixed.push(part)
        end
        let start = if fixed.is_empty() then "." elseif fixed.len() == 1 and fixed[0] == "" then "/" else fixed.join("/") end
        let found: array<string> = []
        let walked = glob_walk(start, pattern, 0, found)
        if walked.is_err() then
            return Result<array<string>, Error>.Err(walked.unwrap_err())
        end
        found.sort()
        Result<array<string>, Error>.Ok(found)
    end
end

--" A text file being read one line at a time (see `fs.lines`). --"
struct Lines
    --" The number of the open file. --"
    public handle: int
end

fun open_lines(path: string) -> Result<Lines, Error>
    let r = fs_open_lines(path)
    if r.is_err() then
        return Result<Lines, Error>.Err(r.unwrap_err())
    end
    Result<Lines, Error>.Ok(Lines { handle: r.unwrap() })
end

fun for_each_line(path: string, f: fun(string)) -> Result<int, Error>
    let opened = open_lines(path)
    if opened.is_err() then
        return Result<int, Error>.Err(opened.unwrap_err())
    end
    let reader = opened.unwrap()
    let count = 0
    while true do
        let next = reader.next()
        if next.is_err() then
            return Result<int, Error>.Err(next.unwrap_err())
        end
        let line = next.unwrap()
        if line == none then
            break
        end
        f(line)
        count += 1
    end
    Result<int, Error>.Ok(count)
end

impl Lines
    --" The next line without its line break, or `none` at the end of the file (it is then closed).
    A line that is not text, or longer than 16 MB, is an `err`. --"
    public fun next(self) -> Result<string?, Error>
        fs_read_line(self.handle)
    end

    --" Closes the file before its end (a file that was read to its end closes by itself). --"
    public fun close(self)
        fs_close_lines(self.handle)
    end
end

--" The kind of what is at `path` as bits (1 file, 2 folder, 4 link), 0 when nothing is there or it cannot be
told. --"
fun kind_at(path: string) -> int
    let st = fs_stat(path)
    if st.is_err() then
        return 0
    end
    let found = st.unwrap()
    if found == none then
        return 0
    end
    found[0]
end

fun stat_of(path: string) -> Result<array<int>, Error>
    let st = fs_stat(path)
    if st.is_err() then
        return Result<array<int>, Error>.Err(st.unwrap_err())
    end
    let found = st.unwrap()
    if found == none then
        return Result<array<int>, Error>.Err(Error { message: "\"" + path + "\" not found" })
    end
    Result<array<int>, Error>.Ok(found)
end

--" A file on disk, by its path (it may not be there yet): `File.of("notes/a.txt")`. The path is read
when a method is called, nothing is kept open. Needs disk access. --"
struct File
    --" The path given to `File.of`. --"
    public path: string
end

impl File
    --" The file at `path`. --"
    public fun of(path: string) -> File
        File { path: path }
    end

    --" The last part of the path: `a.txt`. --"
    public fun name(self) -> string
        Path.base(self.path)
    end

    --" The name without its extension: `a`. --"
    public fun stem(self) -> string
        Path.stem(self.path)
    end

    --" The extension without the dot, in lower case: `txt` (`""` for none). --"
    public fun ext(self) -> string
        Path.ext(self.path)
    end

    --" The folder that holds it. --"
    public fun parent(self) -> Dir
        Dir { path: Path.dir(self.path) }
    end

    --" Whether something is at this path. --"
    public fun exists(self) -> Result<bool, Error>
        fs_exists(self.path)
    end

    --" Whether this is a file (and not a folder). --"
    public fun is_file(self) -> Result<bool, Error>
        let st = fs_stat(self.path)
        if st.is_err() then
            return Result<bool, Error>.Err(st.unwrap_err())
        end
        let found = st.unwrap()
        Result<bool, Error>.Ok(found != none and found[0] % 2 == 1)
    end

    --" Whether this path is a link. --"
    public fun is_link(self) -> Result<bool, Error>
        let st = stat_of(self.path)
        if st.is_err() then
            return Result<bool, Error>.Err(st.unwrap_err())
        end
        Result<bool, Error>.Ok(st.unwrap()[0] >= 4)
    end

    --" The size in bytes. --"
    public fun size(self) -> Result<int, Error>
        let st = stat_of(self.path)
        if st.is_err() then
            return Result<int, Error>.Err(st.unwrap_err())
        end
        Result<int, Error>.Ok(st.unwrap()[1])
    end

    --" When it was last changed (an instant, UTC). --"
    public fun modified(self) -> Result<datetime, Error>
        let st = stat_of(self.path)
        if st.is_err() then
            return Result<datetime, Error>.Err(st.unwrap_err())
        end
        Result<datetime, Error>.Ok(datetime.at(st.unwrap()[2]))
    end

    --" The whole content as text. --"
    public fun read(self) -> Result<string, Error>
        fs_read(self.path)
    end

    --" The whole content as bytes. --"
    public fun read_bytes(self) -> Result<bytes, Error>
        fs_read_bytes(self.path)
    end

    --" The lines, one at a time (see `fs.lines`). --"
    public fun lines(self) -> Result<Lines, Error>
        open_lines(self.path)
    end

    --" Replaces the content. --"
    public fun write(self, content: string) -> Result<bool, Error>
        fs_write(self.path, content)
    end

    --" Adds `content` at the end (creating the file if needed). --"
    public fun append(self, content: string) -> Result<bool, Error>
        fs_append(self.path, content)
    end

    --" Copies the file to `target`; gives the copy. --"
    public fun copy_to(self, target: string) -> Result<File, Error>
        let done = fs_copy(self.path, target)
        if done.is_err() then
            return Result<File, Error>.Err(done.unwrap_err())
        end
        Result<File, Error>.Ok(File { path: target })
    end

    --" Moves (renames) the file to `target`; gives it at its new place. --"
    public fun move_to(self, target: string) -> Result<File, Error>
        let done = fs_rename(self.path, target)
        if done.is_err() then
            return Result<File, Error>.Err(done.unwrap_err())
        end
        Result<File, Error>.Ok(File { path: target })
    end

    --" Deletes the file. --"
    public fun remove(self) -> Result<bool, Error>
        if kind_at(self.path) % 4 >= 2 then
            return Result<bool, Error>.Err(Error { message: "\"" + self.path + "\" is a folder: use Dir.remove" })
        end
        fs_remove(self.path, false)
    end
end

--" A folder on disk, by its path: `Dir.of("src")`. Needs disk access. --"
struct Dir
    --" The path given to `Dir.of`. --"
    public path: string
end

impl Dir
    --" The folder at `path`. --"
    public fun of(path: string) -> Dir
        Dir { path: path }
    end

    --" The last part of the path. --"
    public fun name(self) -> string
        Path.base(self.path)
    end

    --" The folder above. --"
    public fun parent(self) -> Dir
        Dir { path: Path.dir(self.path) }
    end

    --" The path of `name` inside this folder (nothing is read). --"
    public fun join(self, name: string) -> string
        Path.join(self.path, name)
    end

    --" The file called `name` in this folder (it may not be there). --"
    public fun file(self, name: string) -> File
        File { path: Path.join(self.path, name) }
    end

    --" Whether this is a folder that is there. --"
    public fun exists(self) -> Result<bool, Error>
        fs_is_dir(self.path)
    end

    --" Creates the folder and the ones above it that are missing. --"
    public fun create(self) -> Result<bool, Error>
        fs_create_dir(self.path)
    end

    --" Deletes the folder: only an empty one, unless `all` (then all it holds goes too). --"
    public fun remove(self, all: bool = false) -> Result<bool, Error>
        fs_remove(self.path, all)
    end

    --" Moves (renames) the folder to `target`. --"
    public fun move_to(self, target: string) -> Result<Dir, Error>
        let done = fs_rename(self.path, target)
        if done.is_err() then
            return Result<Dir, Error>.Err(done.unwrap_err())
        end
        Result<Dir, Error>.Ok(Dir { path: target })
    end

    --" The files directly in this folder (not those of its sub-folders), sorted by name. --"
    public fun files(self) -> Result<array<File>, Error>
        let names = fs_list(self.path)
        if names.is_err() then
            return Result<array<File>, Error>.Err(names.unwrap_err())
        end
        let out: array<File> = []
        for name in names.unwrap() do
            let path = Path.join(self.path, name)
            if kind_at(path) % 2 == 1 then
                out.push(File { path: path })
            end
        end
        Result<array<File>, Error>.Ok(out)
    end

    --" The folders directly in this folder, sorted by name. --"
    public fun dirs(self) -> Result<array<Dir>, Error>
        let names = fs_list(self.path)
        if names.is_err() then
            return Result<array<Dir>, Error>.Err(names.unwrap_err())
        end
        let out: array<Dir> = []
        for name in names.unwrap() do
            let path = Path.join(self.path, name)
            if kind_at(path) % 4 >= 2 then
                out.push(Dir { path: path })
            end
        end
        Result<array<Dir>, Error>.Ok(out)
    end

    --" The files of this folder and of all the ones below it, one at a time (nothing is read before
    it is needed): a folder's files, then its sub-folders, each sorted by name. `name` keeps the files
    whose name matches (`*.q`), `ignore` skips the files and folders with these names or patterns
    (`[".git", "node_modules"]`), `depth` is how many folder levels to go down (`0`: this folder only). --"
    public fun walk(self, name: string = "*", ignore: array<string> = [], depth: int = 40) -> Result<Iterator<File>, Error>
        let first = fs_list(self.path)
        if first.is_err() then
            return Result<Iterator<File>, Error>.Err(first.unwrap_err())
        end
        let it = WalkIter { pending: [self.path], levels: [0], files: [], name: name, ignore: ignore, depth: depth }
        Result<Iterator<File>, Error>.Ok(it)
    end
end

--" The iterator of `Dir.walk`: the files of a folder and its sub-folders, read as it goes. --"
struct WalkIter
    --" Folders still to read. --"
    public pending: array<string>
    --" The depth of each folder in `pending`. --"
    public levels: array<int>
    --" Files found and not yet given. --"
    public files: array<File>
    --" The pattern the names must match (empty for all). --"
    public name: string
    --" Names of folders to skip. --"
    public ignore: array<string>
    --" How deep to go (-1 for no limit). --"
    public depth: int
end

fun walk_ignored(name: string, ignore: array<string>) -> bool
    for pattern in ignore do
        if Path.matches(pattern, name) then
            return true
        end
    end
    false
end

impl Iterator<File> for WalkIter
    --" The next element, or `Step.End` when there is none. --"
    public fun next(self) -> Step<File>
        while self.files.is_empty() do
            if self.pending.is_empty() then
                return Step<File>.End
            end
            let dir = self.pending.pop() ?? ""
            let level = self.levels.pop() ?? 0
            let names = fs_list(dir)
            if names.is_err() then
                continue
            end
            let subdirs: array<string> = []
            for entry in names.unwrap() do
                if walk_ignored(entry, self.ignore) then
                    continue
                end
                let path = Path.join(dir, entry)
                if kind_at(path) % 4 >= 2 then
                    if level < self.depth then
                        subdirs.push(path)
                    end
                elseif Path.matches(self.name, entry) then
                    self.files.push(File { path: path })
                end
            end
            self.files.reverse()
            -- popped from the end: the last one pushed is the first folder visited
            let i = subdirs.len() - 1
            while i >= 0 do
                self.pending.push(subdirs[i])
                self.levels.push(level + 1)
                i -= 1
            end
        end
        Step<File>.Item(self.files.pop() ?? File { path: "" })
    end
end

fun glob_walk(dir: string, pattern: string, depth: int, found: array<string>) -> Result<bool, Error>
    if depth > 40 or found.len() > 100000 then
        return Result<bool, Error>.Ok(true)
    end
    let names = fs_list(dir)
    if names.is_err() then
        -- a pattern whose folder is not there matches nothing
        return Result<bool, Error>.Ok(true)
    end
    for name in names.unwrap() do
        let full = if dir == "." and not pattern.starts_with("./") then name elseif dir.ends_with("/") then dir + name else dir + "/" + name end
        if Path.matches(pattern, full) then
            found.push(full)
        end
        let sub = fs_is_dir(full)
        if sub.is_ok() and sub.unwrap() then
            let walked = glob_walk(full, pattern, depth + 1, found)
            if walked.is_err() then
                return walked
            end
        end
    end
    Result<bool, Error>.Ok(true)
end

--" File paths written as text, with `/` between the parts (also on Windows). Nothing here touches the
disk: `Path.join("uploads", name)`, `Path.ext("photo.PNG")`. --"
struct Path
end

impl Path
    --" `true` for `/a/b` and `C:/a`. --"
    public fun is_absolute(p: string) -> bool
        p.starts_with("/") or regex.matches(p, "^[A-Za-z]:/")
    end

    --" The shortest way to write the same path: `a//b/./c/../d/` is `a/b/d`, `""` is `.`; a `..` that
    cannot go up stays at the start of a relative path and is dropped at the root. --"
    public fun normalize(p: string) -> string
        if p == "" then
            return "."
        end
        let rooted = p.starts_with("/")
        let drive = ""
        let body = p
        if not rooted and regex.matches(p, "^[A-Za-z]:/") then
            drive = p.substring(0, 2)
            body = p.substring(2, p.len())
            rooted = true
        end
        let stack: array<string> = []
        for part in body.split("/") do
            if part == "" or part == "." then
                continue
            end
            if part == ".." then
                let top = stack.last()
                if top != none and top != ".." then
                    stack.pop()
                else
                    if not rooted then
                        stack.push("..")
                    end
                end
                continue
            end
            stack.push(part)
        end
        let joined = stack.join("/")
        if rooted then
            return drive + "/" + joined
        end
        if joined == "" then "." else joined end
    end

    --" The parts one after the other, normalised; an absolute part starts the path again
    (`join_all(["a", "/b", "c"])` is `/b/c`), empty parts are ignored. --"
    public fun join_all(parts: array<string>) -> string
        let kept = parts.filter(fun(s: string) -> bool s != "" end)
        if kept.is_empty() then
            return ""
        end
        let out = kept[0]
        let i = 1
        while i < kept.len() do
            let next = kept[i]
            if Path.is_absolute(next) then
                out = next
            else
                out = if out.ends_with("/") then out + next else out + "/" + next end
            end
            i += 1
        end
        Path.normalize(out)
    end

    --" Two parts, like `join_all`. --"
    public fun join(a: string, b: string) -> string
        Path.join_all([a, b])
    end

    fun trimmed(p: string) -> string
        let t = p
        while t.len() > 1 and t.ends_with("/") do
            t = t.substring(0, t.len() - 1)
        end
        t
    end

    --" The last part: `/a/b.txt` is `b.txt`, `/a/b/` is `b`, `/` is `/`, `""` is `.`. --"
    public fun base(p: string) -> string
        if p == "" then
            return "."
        end
        let t = Path.trimmed(p)
        if t == "/" then
            return "/"
        end
        let at = t.last_index_of("/")
        if at == none then t else t.substring(at + 1, t.len()) end
    end

    --" Everything but the last part: `/a/b.txt` is `/a`, `b.txt` is `.`, `/a` is `/`. --"
    public fun dir(p: string) -> string
        let t = Path.trimmed(p)
        let at = t.last_index_of("/")
        if at == none then
            return "."
        end
        if at == 0 then
            return "/"
        end
        Path.normalize(t.substring(0, at))
    end

    --" The extension of the last part, without the dot: `a.tar.gz` is `gz`; `""` for `.bashrc`,
    `a.` and a name without a dot. --"
    public fun ext(p: string) -> string
        let name = Path.base(p)
        let dot = name.last_index_of(".")
        if dot == none or dot == 0 or dot == name.len() - 1 then
            return ""
        end
        name.substring(dot + 1, name.len())
    end

    --" The last part without its extension: `a.tar.gz` is `a.tar`, `.bashrc` stays `.bashrc`. --"
    public fun stem(p: string) -> string
        let name = Path.base(p)
        let e = Path.ext(p)
        if e == "" then name else name.substring(0, name.len() - e.len() - 1) end
    end

    --" The path with another extension (`""` removes it): `a/b.txt` and `md` give `a/b.md`. --"
    public fun with_ext(p: string, e: string) -> string
        let old = Path.ext(p)
        let root = if old == "" then p else p.substring(0, p.len() - old.len() - 1) end
        if e == "" then root else root + "." + e end
    end

    fun glob_regex(pattern: string) -> string
        let out = "^"
        let chars = pattern.chars()
        let i = 0
        let n = chars.len()
        while i < n do
            let c = chars[i]
            if c == "*" then
                if i + 1 < n and chars[i + 1] == "*" then
                    if i + 2 < n and chars[i + 2] == "/" then
                        out = out + "(?:.*/)?"
                        i += 3
                    else
                        out = out + ".*"
                        i += 2
                    end
                else
                    out = out + "[^/]*"
                    i += 1
                end
                continue
            end
            if c == "?" then
                out = out + "[^/]"
                i += 1
                continue
            end
            if c == "[" then
                let close = i + 1
                if close < n and (chars[close] == "!" or chars[close] == "^") then
                    close += 1
                end
                if close < n and chars[close] == "]" then
                    close += 1
                end
                while close < n and chars[close] != "]" do
                    close += 1
                end
                if close >= n then
                    out = out + "\\["
                    i += 1
                    continue
                end
                let inner = chars.slice(i + 1, close).join("")
                if inner.starts_with("!") then
                    inner = "^" + inner.substring(1, inner.len())
                end
                out = out + "[" + inner.replace("\\", "\\\\") + "]"
                i = close + 1
                continue
            end
            if "\\.+()|\{\}^$".contains(c) then
                out = out + "\\" + c
            else
                out = out + c
            end
            i += 1
        end
        out + "$"
    end

    --" Whether `text` matches the glob `pattern`: `*` any characters but `/`, `**` any characters
    including `/` (`**/` may match no folder), `?` one character, `[a-c]` / `[!a-c]` a set of characters. --"
    public fun matches(pattern: string, text: string) -> bool
        regex.matches(text, Path.glob_regex(pattern))
    end
end

--" Numbers and text laid out for people: `Format.number(1234567.891, 2)` is `1,234,567.89`,
`Format.bytes(1536)` is `1.5 KB`, `Format.sprintf("%-8s|%05.1f", [Any.of("ab"), Any.of(3.14159)])` is
`ab      |003.1`. --"
struct Format
end

fun digits_with(text: string, thousands: string) -> string
    let out = ""
    let i = text.len()
    while i > 3 do
        out = thousands + text.substring(i - 3, i) + out
        i -= 3
    end
    text.substring(0, i) + out
end

impl Format
    --" The integer with a separator between the thousands: `1234567` and `,` give `1,234,567`. --"
    public fun int_with(n: int, thousands: string) -> string
        let text = "{n}"
        if text.starts_with("-") then
            return "-" + digits_with(text.substring(1, text.len()), thousands)
        end
        digits_with(text, thousands)
    end

    --" The integer with commas between the thousands: `1,234,567`. --"
    public fun int(n: int) -> string
        Format.int_with(n, ",")
    end

    --" `n` with exactly `decimals` digits after the point (0 to 15, rounded), `point` before them and
    `thousands` between the groups of three digits of the integer part. A number too large for that
    (more than about 9e18 once scaled) is written as it is. --"
    public fun number_with(n: float, decimals: int, point: string, thousands: string) -> string
        let d = if decimals < 0 then 0 elseif decimals > 15 then 15 else decimals end
        if n != n then
            return "NaN"
        end
        let scaled_float = n.abs() * (10.0 ** d)
        if scaled_float >= 9000000000000000000.0 then
            return "{n}"
        end
        let scaled = scaled_float.round()
        let text = "{scaled}"
        if text.len() <= d then
            text = "0".repeat(d + 1 - text.len()) + text
        end
        let whole = digits_with(text.substring(0, text.len() - d), thousands)
        let out = if d == 0 then whole else whole + point + text.substring(text.len() - d, text.len()) end
        if n < 0.0 and scaled != 0 then "-" + out else out end
    end

    --" `n` with `decimals` digits after the point and commas between the thousands: `1,234.50`. --"
    public fun number(n: float, decimals: int) -> string
        Format.number_with(n, decimals, ".", ",")
    end

    --" A size in bytes for a person: `512 B`, `1.5 KB`, `3.2 MB` (units of 1024). --"
    public fun bytes(n: int) -> string
        if n < 1024 and n > -1024 then
            return "{n} B"
        end
        let units = ["KB", "MB", "GB", "TB", "PB"]
        let value = n as float
        let i = -1
        while (value >= 1024.0 or value <= -1024.0) and i < 4 do
            value = value / 1024.0
            i += 1
        end
        Format.number(value, 1) + " " + units[i]
    end

    fun pad(text: string, width: int, left: bool, fill: string) -> string
        if text.len() >= width then
            return text
        end
        if left then text.pad_left(width, fill) else text.pad_right(width, fill) end
    end

    fun sprintf_fail(message: string)
        panic("Format.sprintf: {message}")
    end

    --" A text from a template: `%s` a text (any value is written as it prints), `%d` an integer, `%f` a
    number with `.2` decimals, `%x` `%X` `%o` `%b` an integer in base 16, 8 or 2, `%%` a percent sign. After the `%`: `-` (pad on the right), `0` (pad with zeros), `+` (always a
    sign), a width, `.precision` (decimals for `%f`, longest text for `%s`). The values come as an array
    of `Any`; a value that does not fit its `%` or a missing one is a panic, like a wrong index: it is a
    mistake of the program. --"
    public fun sprintf(template: string, args: array<Any>) -> string
        let chars = template.chars()
        let n = chars.len()
        let out = ""
        let i = 0
        let used = 0
        while i < n do
            let c = chars[i]
            if c != "%" then
                out = out + c
                i += 1
                continue
            end
            i += 1
            if i >= n then
                Format.sprintf_fail("the template ends after a `%`")
            end
            if chars[i] == "%" then
                out = out + "%"
                i += 1
                continue
            end
            let left = false
            let zero = false
            let plus = false
            while i < n and (chars[i] == "-" or chars[i] == "0" or chars[i] == "+") do
                if chars[i] == "-" then
                    left = true
                end
                if chars[i] == "0" then
                    zero = true
                end
                if chars[i] == "+" then
                    plus = true
                end
                i += 1
            end
            let width = 0
            while i < n and chars[i].is_digit() do
                width = width * 10 + (int.parse(chars[i]) ?? 0)
                i += 1
            end
            let precision = -1
            if i < n and chars[i] == "." then
                i += 1
                precision = 0
                while i < n and chars[i].is_digit() do
                    precision = precision * 10 + (int.parse(chars[i]) ?? 0)
                    i += 1
                end
            end
            if i >= n then
                Format.sprintf_fail("the template ends inside a `%`")
            end
            let verb = chars[i]
            i += 1
            if used >= args.len() then
                Format.sprintf_fail("`%{verb}` has no value (the array has {args.len()})")
            end
            let arg = args[used]
            used += 1
            let body = ""
            let numeric = false
            if verb == "s" then
                let text = arg.as<string>() ?? "{arg}"
                body = if precision >= 0 and text.len() > precision then text.substring(0, precision) else text end
            elseif verb == "d" then
                let v = arg.as<int>()
                if v == none then
                    Format.sprintf_fail("`%d` needs an int, got {arg}")
                end
                body = "{v}"
                if plus and (v ?? 0) >= 0 then
                    body = "+" + body
                end
                numeric = true
            elseif verb == "f" then
                let f = arg.as<float>()
                let as_int = arg.as<int>()
                let v = if f != none then f else if as_int != none then as_int as float else none end end
                if v == none then
                    Format.sprintf_fail("`%f` needs a number, got {arg}")
                end
                body = Format.number_with(v ?? 0.0, if precision < 0 then 6 else precision end, ".", "")
                if plus and (v ?? 0.0) >= 0.0 then
                    body = "+" + body
                end
                numeric = true
            elseif verb == "x" or verb == "X" or verb == "o" or verb == "b" then
                let v = arg.as<int>()
                if v == none then
                    Format.sprintf_fail("`%{verb}` needs an int, got {arg}")
                end
                let value = v ?? 0
                body = if verb == "o" then value.to_oct() elseif verb == "b" then value.to_bin() else value.to_hex() end
                if verb == "X" then
                    body = body.upper()
                end
                numeric = true
            else
                Format.sprintf_fail("`%{verb}` is not a known conversion (s d f x X o b)")
            end
            if numeric and zero and not left and body.len() < width then
                let sign = if body.starts_with("-") or body.starts_with("+") then body.substring(0, 1) else "" end
                let rest = body.substring(sign.len(), body.len())
                body = sign + "0".repeat(width - body.len()) + rest
            end
            out = out + Format.pad(body, width, not left, " ")
        end
        if used < args.len() then
            Format.sprintf_fail("{args.len() - used} value(s) were not used")
        end
        out
    end
end

-- The host provides secure randomness and password hashing.
extern fun secure_random_bytes(n: int) -> Result<bytes, Error>
extern fun constant_time_eq(a: bytes, b: bytes) -> bool
extern fun password_hash(plain: string) -> Result<string, Error>
extern fun password_verify(plain: string, hash: string) -> Result<bool, Error>
extern fun password_needs_rehash(hash: string) -> Result<bool, Error>

-- The host writes the program's log (every host has this one).
extern fun log_write(level: string, message: string, fields: map<string, string>) -> bool

--" The program's log, with the levels of PSR-3: `Log.info("started")`, and a context of any
value (a map, a structure, an array...) written as JSON: `Log.error("payment failed", {"order":
42})`. From the lowest: `debug`, `info`, `notice`, `warning` (or `warn`), `error`, `critical`,
`alert`, `emergency`. The `qlang` command writes each line on its standard error (`--log-level
warning` hides the lower ones, `--log-format json` writes JSON, the context as an object under
`context`); another host may keep the lines or drop them. --"
struct Log
end

impl Log
    --" Writes `message` at the level `level` (any other level is written as `info`) with the
    `context`, a value of any type written as JSON. --"
    public fun log<C>(level: string, message: string, context: C? = none)
        let known = ["debug", "info", "notice", "warning", "warn", "error", "critical", "alert", "emergency"].contains(level)
        let named = if known then level else "info" end
        if context == none then
            log_write(named, message, {})
        else
            log_write(named, message, { "context": Json.of(context).to_text() })
        end
    end

    --" Writes `message` at the level `level` with extra `name: value` pairs. --"
    public fun write(level: string, message: string, fields: map<string, string>)
        let known = ["debug", "info", "notice", "warning", "warn", "error", "critical", "alert", "emergency"].contains(level)
        log_write(if known then level else "info" end, message, fields)
    end

    --" Detailed information for whoever looks for a problem. --"
    public fun debug<C>(message: string, context: C? = none)
        Log.log("debug", message, context)
    end

    --" Something that happened and is worth knowing. --"
    public fun info<C>(message: string, context: C? = none)
        Log.log("info", message, context)
    end

    --" A normal but significant event. --"
    public fun notice<C>(message: string, context: C? = none)
        Log.log("notice", message, context)
    end

    --" Something odd that the program survives. --"
    public fun warning<C>(message: string, context: C? = none)
        Log.log("warning", message, context)
    end

    --" Something that failed, but the program goes on. --"
    public fun error<C>(message: string, context: C? = none)
        Log.log("error", message, context)
    end

    --" A component that is not available, an exception nobody expected. --"
    public fun critical<C>(message: string, context: C? = none)
        Log.log("critical", message, context)
    end

    --" Someone must act at once (the database is down). --"
    public fun alert<C>(message: string, context: C? = none)
        Log.log("alert", message, context)
    end

    --" The system cannot be used. --"
    public fun emergency<C>(message: string, context: C? = none)
        Log.log("emergency", message, context)
    end
end

--" Secrets: random bytes nobody can guess, and a comparison that does not leak where two
secrets differ. Needs a source of secure randomness (the `qlang` command has one). --"
struct Crypto
end

impl Crypto
    --" `n` bytes (0 to 1048576) from the operating system's secure source: never the seeded
    generator of `random`. --"
    public fun random_bytes(n: int) -> Result<bytes, Error>
        secure_random_bytes(n)
    end

    --" A token for a session, a reset link or an API key: `n` secure random bytes written in
    URL-safe base64 (32 bytes give 43 characters). --"
    public fun token(n: int) -> Result<string, Error>
        let r = secure_random_bytes(n)
        if r.is_err() then
            return Result<string, Error>.Err(r.unwrap_err())
        end
        Result<string, Error>.Ok(Base64.encode_url_bytes(r.unwrap()))
    end

    --" `true` if the two texts are equal. It takes the same time wherever they differ, so it is the
    way to compare a token, a signature or a hash with one received. --"
    public fun equal(a: string, b: string) -> bool
        constant_time_eq(a.to_bytes(), b.to_bytes())
    end

    --" `true` if `signature` is the signature of `message` made with the private key that goes with
    `key`, a public key as a PEM (`BEGIN PUBLIC KEY`, `BEGIN RSA PUBLIC KEY`) or a JWK (JSON text).
    `alg` is `RS256`, `RS384`, `RS512` (RSA PKCS#1), `PS256`, `PS384`, `PS512` (RSA-PSS), `ES256`, `ES384`
    (the signature is `r` then `s`, as a JWT writes it) or `EdDSA` (Ed25519); with `HS256`, `HS384` or
    `HS512` the `key` is a shared secret (HMAC, compared in constant time; an empty secret is an error). A wrong signature is `false`;
    `err` is for a key or an algorithm that cannot be used. Needs the `qlang` command. --"
    public fun verify(alg: string, key: string, message: bytes, signature: bytes) -> Result<bool, Error>
        crypto_verify(alg, key, message, signature)
    end

    --" The signature of `message` with a private key: a PEM in PKCS#8 (`BEGIN PRIVATE KEY`; for RSA
    also `BEGIN RSA PRIVATE KEY`), or the secret itself for `HS256`, `HS384`, `HS512`. Same algorithms as
    `verify`. Needs the `qlang` command. --"
    public fun sign(alg: string, key: string, message: bytes) -> Result<bytes, Error>
        crypto_sign(alg, key, message)
    end

    --" Like `equal` for bytes. --"
    public fun equal_bytes(a: bytes, b: bytes) -> bool
        constant_time_eq(a, b)
    end
end

--" Passwords: `Password.hash` makes a text to store (Argon2id, with its own random salt and
settings inside), `Password.verify` checks a password against it. Never store the password
itself. Only the `qlang` command offers it. --"
struct Password
end

impl Password
    --" The hash of `plain`, to keep in the database. Slow on purpose (tens of milliseconds). --"
    public fun hash(plain: string) -> Result<string, Error>
        password_hash(plain)
    end

    --" `true` if `plain` is the password `hash` was made from; `err` if `hash` is not a hash. --"
    public fun verify(plain: string, hash: string) -> Result<bool, Error>
        password_verify(plain, hash)
    end

    --" `true` if `hash` was made with weaker settings than today's: hash the password again, after
    a successful `verify`, and store the new one. --"
    public fun needs_rehash(hash: string) -> Result<bool, Error>
        password_needs_rehash(hash)
    end
end

--" The process environment: `env.get(name)` and `env.args()`. Needs a process environment. --"
struct env
end

impl env
    --" The value of the environment variable `name`, or `none` if it is not set. Needs a process environment. --"
    public fun get(name: string) -> string?
        env_get(name)
    end

    --" The program's own command-line arguments. Needs a process environment. --"
    public fun args() -> array<string>
        env_args()
    end
end

--" The network: `http.send(request)`. Needs network access. --"
struct http
end

impl http
    --" Sends an HTTP request and waits for the response. Needs network access. A connection failure is `err`, not a crash. --"
    public fun send(req: Request) -> Result<Response, Error>
        http_send(req)
    end

    --" Sends several requests **at the same time** and gives the answers in the same order, so
    ten calls to slow services take as long as the slowest one. Each answer is a `Result` like
    that of `send`: a request that failed (no connection, timeout) is an `err` and does not stop
    the others. At most 16 run at once. Needs network access. --"
    public fun send_all(reqs: array<Request>) -> array<Result<Response, Error>>
        let out: array<Result<Response, Error>> = []
        let answers = http_send_all(reqs)
        if answers.is_err() then
            for r in reqs do
                out.push(Result<Response, Error>.Err(answers.unwrap_err()))
            end
            return out
        end
        for a in answers.unwrap() do
            if a.status == 0 then
                out.push(Result<Response, Error>.Err(Error { message: a.body }))
            else
                out.push(Result<Response, Error>.Ok(a))
            end
        end
        out
    end
end

--" Stops the program (a failed test) unless `actual == expected`, showing both values. --"
fun assert_eq<T: Eq>(actual: T, expected: T)
    if actual != expected then
        panic("assert_eq failed\n  actual:   {actual}\n  expected: {expected}")
    end
end

--" Stops the program (a failed test) if `actual == unexpected`, showing the value. --"
fun assert_ne<T: Eq>(actual: T, unexpected: T)
    if actual == unexpected then
        panic("assert_ne failed: both sides are {actual}")
    end
end

--" Stops the program (a failed test) unless `actual < limit`, showing both values. --"
fun assert_lt<T: Ord>(actual: T, limit: T)
    if not (actual < limit) then
        panic("assert_lt failed: {actual} is not less than {limit}")
    end
end

--" Stops the program (a failed test) unless `actual <= limit`, showing both values. --"
fun assert_le<T: Ord>(actual: T, limit: T)
    if not (actual <= limit) then
        panic("assert_le failed: {actual} is not less than or equal to {limit}")
    end
end

--" Stops the program (a failed test) unless `actual > limit`, showing both values. --"
fun assert_gt<T: Ord>(actual: T, limit: T)
    if not (actual > limit) then
        panic("assert_gt failed: {actual} is not greater than {limit}")
    end
end

--" Stops the program (a failed test) unless `actual >= limit`, showing both values. --"
fun assert_ge<T: Ord>(actual: T, limit: T)
    if not (actual >= limit) then
        panic("assert_ge failed: {actual} is not greater than or equal to {limit}")
    end
end

--" Stops the program (a failed test) unless `items` has `expected` elements. --"
fun assert_len<T>(items: array<T>, expected: int)
    if items.len() != expected then
        panic("assert_len failed: {items} has {items.len()} element(s), expected {expected}")
    end
end

--" Stops the program (a failed test) unless `items` has no element. --"
fun assert_empty<T>(items: array<T>)
    if items.len() != 0 then
        panic("assert_empty failed: got {items}")
    end
end

--" Stops the program (a failed test) if `item` is in `items`. --"
fun assert_not_contains<T: Eq>(items: array<T>, item: T)
    if items.contains(item) then
        panic("assert_not_contains failed: {items} contains {item}")
    end
end

--" Stops the program (a failed test) unless `text` starts with `prefix`. --"
fun assert_starts_with(text: string, prefix: string)
    if not text.starts_with(prefix) then
        panic("assert_starts_with failed: \"{text}\" does not start with \"{prefix}\"")
    end
end

--" Stops the program (a failed test) unless `text` ends with `suffix`. --"
fun assert_ends_with(text: string, suffix: string)
    if not text.ends_with(suffix) then
        panic("assert_ends_with failed: \"{text}\" does not end with \"{suffix}\"")
    end
end

--" A set: values without duplicates, kept in the order they were added. The values follow
the rules of map keys (`int`, `float`, `string`, `bool`, `uuid`, `ulid`, `datetime`, an enum
whose variants carry no data). Make one with `Set<int>.new()` or `Set.of([1, 2, 2, 3])`,
go through it with `for x in s`. --"
struct Set<T>
    items: map<T, bool>
end

impl<T> Set<T>
    --" An empty set: `Set<int>.new()`. --"
    public fun new() -> Set<T>
        let m: map<T, bool> = {}
        Set { items: m }
    end

    --" A set with the values of `values`, without the duplicates. --"
    public fun of(values: array<T>) -> Set<T>
        let s = Set<T>.new()
        for v in values do
            s.add(v)
        end
        s
    end

    --" Adds `value`; whether it was new. --"
    public fun add(self, value: T) -> bool
        if self.items.has(value) then
            return false
        end
        self.items[value] = true
        true
    end

    --" Whether `value` is in the set. --"
    public fun contains(self, value: T) -> bool
        self.items.has(value)
    end

    --" Removes `value`; whether it was there. --"
    public fun remove(self, value: T) -> bool
        self.items.remove(value) != none
    end

    --" The number of values. --"
    public fun len(self) -> int
        self.items.len()
    end

    --" Whether the set has no value. --"
    public fun is_empty(self) -> bool
        self.items.len() == 0
    end

    --" Removes every value. --"
    public fun clear(self)
        self.items.clear()
    end

    --" The values, in the order they were added. --"
    public fun to_array(self) -> array<T>
        self.items.keys()
    end

    --" A new set with the values of both sets. --"
    public fun union(self, other: Set<T>) -> Set<T>
        let out = Set<T>.of(self.to_array())
        for v in other do
            out.add(v)
        end
        out
    end

    --" A new set with the values that are in both sets. --"
    public fun intersect(self, other: Set<T>) -> Set<T>
        let out = Set<T>.new()
        for v in self do
            if other.contains(v) then
                out.add(v)
            end
        end
        out
    end

    --" A new set with the values of this set that are not in `other`. --"
    public fun difference(self, other: Set<T>) -> Set<T>
        let out = Set<T>.new()
        for v in self do
            if not other.contains(v) then
                out.add(v)
            end
        end
        out
    end

    --" Whether every value of this set is in `other`. --"
    public fun is_subset(self, other: Set<T>) -> bool
        for v in self do
            if not other.contains(v) then
                return false
            end
        end
        true
    end

    --" Whether both sets have the same values (whatever their order). --"
    public fun equals(self, other: Set<T>) -> bool
        self.len() == other.len() and self.is_subset(other)
    end
end

impl<T> As<string> for Set<T>
    --" The set as text: `Set(1, 2, 3)`. --"
    public fun convert(self) -> string
        let parts: array<string> = []
        for v in self do
            parts[] = "{v}"
        end
        let body = parts.join(", ")
        "Set({body})"
    end
end

--" The value of an ok `Result`; stops the program (a failed test) showing the error otherwise. --"
fun assert_ok<T, E>(result: Result<T, E>) -> T
    match result
        case Ok(v) then v
        case Err(e) then panic("assert_ok failed: got the error {e}")
    end
end

--" The error of an error `Result`; stops the program (a failed test) showing the value if it is ok. --"
fun assert_err<T, E>(result: Result<T, E>) -> E
    match result
        case Ok(v) then panic("assert_err failed: got the ok value {v}")
        case Err(e) then e
    end
end

--" The value, which must not be `none`; stops the program (a failed test) otherwise. --"
fun assert_some<T>(value: T?) -> T
    let v = value
    if v != none then
        v
    else
        panic("assert_some failed: the value is none")
    end
end

--" Stops the program (a failed test) unless the value is `none`. --"
fun assert_none<T>(value: T?)
    if value != none then
        panic("assert_none failed: got {value}")
    end
end

--" Stops the program (a failed test) unless `item` is in `items`. --"
fun assert_contains<T: Eq>(items: array<T>, item: T)
    if not items.contains(item) then
        panic("assert_contains failed: {items} does not contain {item}")
    end
end

--" Stops the program (a failed test) unless `text` contains `part`. --"
fun assert_contains_text(text: string, part: string)
    if not text.contains(part) then
        panic("assert_contains_text failed: \"{text}\" does not contain \"{part}\"")
    end
end

--" Stops the program (a failed test) unless `actual` is within `tolerance` of `expected`
(relative to `expected` once it is larger than 1): for numbers that went through float
arithmetic. --"
fun assert_within(actual: float, expected: float, tolerance: float)
    let scale = expected.abs().max(1.0)
    if (actual - expected).abs() > tolerance * scale then
        panic("assert_within failed: {actual} is not within {tolerance} of {expected}")
    end
end

--" Like `assert_within` with a tolerance of one billionth. --"
fun assert_close(actual: float, expected: float)
    assert_within(actual, expected, 0.000000001)
end

-- Signatures with a key pair: only the `qlang` command offers them.
extern fun crypto_verify(alg: string, key: string, message: bytes, signature: bytes) -> Result<bool, Error>
extern fun crypto_sign(alg: string, key: string, message: bytes) -> Result<bytes, Error>

-- Whether the process was asked to stop (Ctrl-C, `kill`): only the `qlang` command.
extern fun process_stopping() -> bool

-- Starting programs: only where the host allows it.
extern fun process_run(command: string, args: array<string>, input: string?) -> Result<ProcessOutput, Error>

--" What a program wrote on its two output streams, and the code it ended with (-1 when a
signal ended it). --"
struct ProcessOutput
    --" The exit code of the program. --"
    public status: int
    --" What it wrote on its standard output. --"
    public stdout: string
    --" What it wrote on its standard error. --"
    public stderr: string
end

--" Starting programs: `Process.run("git", ["status"])`. No shell is involved, so the
arguments are passed as they are. Needs permission (`--allow-process` in the terminal); a
program is not confined by `--root`, and one that runs for more than a minute is stopped. --"
struct Process
end

impl Process
    --" Whether the program was asked to stop (Ctrl-C, `kill`, a service manager). The first call
    makes the program take over: from then on the signal no longer ends it at once, it only makes
    this return `true`, so that a worker can finish what it is doing and leave. A second signal
    ends the program for good. In a `qlang serve` program it is `true` while the server stops.
    Needs the `qlang` command. --"
    public fun stopping() -> bool
        process_stopping()
    end

    --" Runs `command` with `args` and waits for it. `err` if it cannot be started; a program
    that ends with a non-zero `status` is still an `ok` result. --"
    public fun run(command: string, args: array<string>) -> Result<ProcessOutput, Error>
        process_run(command, args, none)
    end

    --" Like `run`, with `input` written to the program's standard input. --"
    public fun run_with_input(command: string, args: array<string>, input: string) -> Result<ProcessOutput, Error>
        process_run(command, args, input)
    end
end

--" What `Panic.guard` caught: a failed `assert`, a `panic`, an `unwrap` of an error, an index out of
range... `places` are `file:line` pieces, the innermost first: where it happened, then the calls that
led there (up to 8). --"
struct Caught
    --" The code of the error (`R011`). --"
    public code: string
    --" The message of the error. --"
    public message: string
    --" Where it happened, innermost first (`file:line`). --"
    public places: array<string>
end

--" Runs a function so that a panic in it does not stop the program (for a test runner, or to keep a
batch going). Running out of steps or memory is not caught.

```
let failure = Panic.guard(fun() assert_eq(1 + 1, 3) end)
if failure != none then
  print(failure.message)            -- assertion failed...
end
```

The state the function changed (variables it reached, files, the database) stays as it was left. --"
struct Panic
end

impl Panic
    --" `none` when `f` finished, else what stopped it. --"
    public fun guard(f: fun()) -> Caught?
        let caught = panic_guard(f)
        if caught == none then
            return none
        end
        let parts = caught
        Caught { code: parts[0], message: parts[1], places: parts.slice(2, parts.len()) }
    end
end

-- The `qlang` command provides the tasks.
extern fun task_spawn(file: string, input: string) -> Result<int, Error>
extern fun task_send(id: int, message: string) -> Result<bool, Error>
extern fun task_recv(id: int, timeout_ms: int) -> Result<string?, Error>
extern fun task_wait(id: int, timeout_ms: int) -> Result<string?, Error>
extern fun task_cancel(id: int) -> bool
extern fun task_done(id: int) -> bool
extern fun task_input() -> string
extern fun task_emit(message: string) -> Result<bool, Error>
extern fun task_next(timeout_ms: int) -> string?
extern fun task_cancelled() -> bool
extern fun task_finish(result: string)

--" Work that runs **at the same time** as your program, in a task of its own: a file of yours run by an
interpreter that shares nothing with the program (the model of the workers of `qlang serve`). The two talk by
sending `Json` values. Needs the `qlang` command; at most 64 tasks at once.

The file of the task has a `fun run()`:

```
-- square.q
fun run()
  let n = (Worker.input().get("n") ?? Json.null()).as_number()
  Worker.finish(Json.number(n * n))
end
```

and the program starts it and waits:

```
let w = Worker.spawn("square.q", Json.of({"n": 12})).unwrap()
let result = w.wait(5000).unwrap()                 -- 144.0, or none if it is not over after 5 s
```

A task can also talk while it works. In the task: `Worker.emit(value)` sends to the program, `Worker.next(ms)`
waits for what the program sent (`none` after `ms`), `Worker.cancelled()` says whether it was asked to stop
(by `cancel()`, or because the server is stopping). In the program: `w.recv(ms)`, `w.send(value)`,
`w.cancel()`, `w.is_done()`. `cancel()` also **stops the task at its next few thousand steps** (error
`R906`), even in a loop that never looks at `cancelled()`; a task waiting in a host call stops when it returns.

The file is looked for from the folder you run `qlang` in (inside `--root` if there is one). The task has the
permissions of the program (`--allow-process`...), its own memory and its own database connections, and it
runs the top of its file again. `wait` gives the result **once**: a task is forgotten after it. A task that
fails (an error in its file, `R906`) makes `wait` an `err` with the message. --"
struct Worker
    --" The number of the task. --"
    public id: int
end

impl Worker
    --" Starts the file `file` (its `fun run()`) with `input`, which the task reads with `Worker.input()`. --"
    public fun spawn(file: string, input: Json) -> Result<Worker, Error>
        let made = task_spawn(file, input.to_text())
        if made.is_err() then
            return Result<Worker, Error>.Err(made.unwrap_err())
        end
        Result<Worker, Error>.Ok(Worker { id: made.unwrap() })
    end

    --" Sends a value to the task (it reads it with `Worker.next`). --"
    public fun send(self, message: Json) -> Result<bool, Error>
        task_send(self.id, message.to_text())
    end

    --" The next value the task emitted, waiting at most `timeout_ms`; `none` if there is none. --"
    public fun recv(self, timeout_ms: int) -> Result<Json?, Error>
        let got = task_recv(self.id, timeout_ms)
        if got.is_err() then
            return Result<Json?, Error>.Err(got.unwrap_err())
        end
        let text = got.unwrap()
        if text == none then
            return Result<Json?, Error>.Ok(none)
        end
        let parsed = Json.parse(text)
        if parsed.is_err() then
            return Result<Json?, Error>.Err(parsed.unwrap_err())
        end
        Result<Json?, Error>.Ok(parsed.unwrap())
    end

    --" The result of the task (what it gave to `Worker.finish`, `null` if nothing), waiting at most
    `timeout_ms`: `none` when it is still running. An `err` if the task failed or its result was already taken. --"
    public fun wait(self, timeout_ms: int) -> Result<Json?, Error>
        let got = task_wait(self.id, timeout_ms)
        if got.is_err() then
            return Result<Json?, Error>.Err(got.unwrap_err())
        end
        let text = got.unwrap()
        if text == none then
            return Result<Json?, Error>.Ok(none)
        end
        let parsed = Json.parse(text)
        if parsed.is_err() then
            return Result<Json?, Error>.Err(parsed.unwrap_err())
        end
        Result<Json?, Error>.Ok(parsed.unwrap())
    end

    --" Asks the task to stop: `Worker.cancelled()` becomes true for it, and it is stopped at its next few
    thousand steps. --"
    public fun cancel(self)
        task_cancel(self.id)
    end

    --" Whether the task is over (its result may still wait to be taken with `wait`). --"
    public fun is_done(self) -> bool
        task_done(self.id)
    end

    --" In a task: what the program gave to `spawn`. --"
    public fun input() -> Json
        let parsed = Json.parse(task_input())
        if parsed.is_err() then
            return Json.null()
        end
        parsed.unwrap()
    end

    --" In a task: sends a value to the program (it reads it with `recv`). --"
    public fun emit(message: Json) -> Result<bool, Error>
        task_emit(message.to_text())
    end

    --" In a task: the next value the program sent, waiting at most `timeout_ms`; `none` if there is none. --"
    public fun next(timeout_ms: int) -> Json?
        let text = task_next(timeout_ms)
        if text == none then
            return none
        end
        let parsed = Json.parse(text)
        if parsed.is_err() then
            return none
        end
        parsed.unwrap()
    end

    --" In a task: whether it was asked to stop (`cancel`, or the server is stopping). --"
    public fun cancelled() -> bool
        task_cancelled()
    end

    --" In a task: sets the result `wait` will give. --"
    public fun finish(result: Json)
        task_finish(result.to_text())
    end
end

-- The `qlang` command provides the numbers shared by the workers.
extern fun shared_add(name: string, delta: int) -> Result<int, Error>
extern fun shared_get(name: string) -> int
extern fun shared_set(name: string, value: int) -> Result<int, Error>
extern fun shared_cas(name: string, expected: int, new: int) -> Result<bool, Error>

--" Whole numbers that **every worker** of the process sees and changes atomically: exact counters
(requests served, jobs in progress, a limit shared by all the workers) without going through Redis.
They live in the memory of the process, so they are lost when it stops and they are not shared between
machines (for that, use Redis). A name is from 1 to 128 bytes, at most 10 000 names; a name never written
holds 0. Needs the `qlang` command.

```
let n = Shared.add("requests", 1).unwrap()          -- 1, 2, 3... even with 8 workers
Shared.set("limit", 100).unwrap()
if Shared.compare_set("leader", 0, 1).unwrap() then   -- only one worker gets true
  print("I start the cleaning")
end
```
--"
struct Shared
end

impl Shared
    --" Adds `delta` (it can be negative) and gives the new value. --"
    public fun add(name: string, delta: int) -> Result<int, Error>
        shared_add(name, delta)
    end

    --" The value (0 for a name never written). --"
    public fun get(name: string) -> int
        shared_get(name)
    end

    --" Sets the value and gives it. --"
    public fun set(name: string, value: int) -> Result<int, Error>
        shared_set(name, value)
    end

    --" Sets `new` only if the value is `expected`, in one step: `true` when it did. --"
    public fun compare_set(name: string, expected: int, new: int) -> Result<bool, Error>
        shared_cas(name, expected, new)
    end
end

--" Distributed tracing (OpenTelemetry) of `qlang serve --otlp-endpoint URL`: the request is a span of a
trace (it continues the `traceparent` the client sent), the HTTP calls the program makes are spans too and carry
`traceparent`, and every log line of the request has `trace_id` and `span_id`. `Trace.span(name, f)` makes a
span of your own around `f`; `Trace.attr` adds a note to the span being run; `Trace.id()` is the trace id (to show
on an error page, for instance). Outside a traced request all of these do nothing (`id()` is the empty text).
Needs the `qlang` command. --"
struct Trace
end

impl Trace
    --" Runs `f` as a span called `name` and gives what it gives. --"
    public fun span<T>(name: string, f: fun() -> T) -> T
        let handle = trace_start(name)
        let result = f()
        trace_end(handle, false)
        result
    end

    --" Adds `key = value` to the span being run. --"
    public fun attr(key: string, value: string)
        trace_attr(key, value)
    end

    --" The id of the trace of this request (32 hexadecimal digits), or "" outside a traced request. --"
    public fun id() -> string
        trace_id()
    end

    --" The `traceparent` header value to give to a service called by other means than `http` (a queue message, for example). --"
    public fun traceparent() -> string
        trace_traceparent()
    end
end

extern fun trace_start(name: string) -> int
extern fun trace_end(handle: int, failed: bool)
extern fun trace_id() -> string
extern fun trace_traceparent() -> string
extern fun trace_attr(key: string, value: string)

-- The `qlang` command provides compression and the time zone database.
extern fun archive_gzip(data: bytes, level: int) -> bytes
extern fun archive_gunzip(data: bytes, max: int) -> Result<bytes, Error>
extern fun archive_read(kind: string, data: bytes, max: int) -> Result<array<bytes>, Error>
extern fun archive_write(kind: string, parts: array<bytes>) -> Result<bytes, Error>
extern fun tz_offset(zone: string, ms: int) -> Result<array<string>, Error>
extern fun tz_resolve(zone: string, wall: int) -> Result<int, Error>

--" A file of an archive: its path inside the archive (`docs/a.txt`, never absolute and never with
`..`) and its content. --"
struct ArchiveEntry
    --" The path of the entry in the archive. --"
    public name: string
    --" Its content. --"
    public data: bytes
end

fun archive_entries(read: Result<array<bytes>, Error>) -> Result<array<ArchiveEntry>, Error>
    if read.is_err() then
        return Result<array<ArchiveEntry>, Error>.Err(read.unwrap_err())
    end
    let parts = read.unwrap()
    let out: array<ArchiveEntry> = []
    let i = 0
    while i + 1 < parts.len() do
        out.push(ArchiveEntry { name: parts[i].to_string() ?? "", data: parts[i + 1] })
        i += 2
    end
    Result<array<ArchiveEntry>, Error>.Ok(out)
end

fun archive_parts(entries: array<ArchiveEntry>) -> array<bytes>
    let parts: array<bytes> = []
    for e in entries do
        parts.push(e.name.to_bytes())
        parts.push(e.data)
    end
    parts
end

--" gzip compression of bytes (`.gz` files, `Content-Encoding: gzip`). Needs the `qlang` command.

```
let packed = Gzip.compress("hello".to_bytes())
let text = Gzip.decompress(packed).unwrap().to_string()      -- "hello"
```

`decompress` refuses to produce more than 256 MB (a small file can expand into gigabytes);
`decompress_max` chooses the limit. --"
struct Gzip
end

impl Gzip
    --" Compresses `data` (level 6). --"
    public fun compress(data: bytes) -> bytes
        archive_gzip(data, 6)
    end

    --" Compresses with a level from 0 (none) to 9 (smallest). --"
    public fun compress_level(data: bytes, level: int) -> bytes
        archive_gzip(data, level)
    end

    --" Decompresses; `err` for data that is not gzip or that is over 256 MB once decompressed. --"
    public fun decompress(data: bytes) -> Result<bytes, Error>
        archive_gunzip(data, 268435456)
    end

    --" Like `decompress`, with the largest size you accept, in bytes. --"
    public fun decompress_max(data: bytes, max: int) -> Result<bytes, Error>
        archive_gunzip(data, max)
    end
end

--" `.zip` archives, in memory. Needs the `qlang` command. A name that would leave the folder it is
extracted into (`../x`, `/etc/passwd`) is refused when reading and when writing, and the content
is limited to 256 MB once decompressed (`read_max` chooses).

```
let packed = Zip.write([ArchiveEntry { name: "hello.txt", data: "hi".to_bytes() }]).unwrap()
for e in Zip.read(packed).unwrap() do
  print("{e.name}: {e.data.len()} bytes")
end
```
--"
struct Zip
end

impl Zip
    --" The files of a zip archive (folders are left out). --"
    public fun read(data: bytes) -> Result<array<ArchiveEntry>, Error>
        archive_entries(archive_read("zip", data, 268435456))
    end

    --" Like `read`, with the largest total size you accept, in bytes. --"
    public fun read_max(data: bytes, max: int) -> Result<array<ArchiveEntry>, Error>
        archive_entries(archive_read("zip", data, max))
    end

    --" A zip archive of these files. --"
    public fun write(entries: array<ArchiveEntry>) -> Result<bytes, Error>
        archive_write("zip", archive_parts(entries))
    end
end

--" `.tar` and `.tar.gz` archives, in memory. Needs the `qlang` command. `read` recognises a gzip
file by its first bytes, so it reads both. Same rules on names and size as `Zip`. --"
struct Tar
end

impl Tar
    --" The files of a tar archive, gzipped or not (folders and links are left out). --"
    public fun read(data: bytes) -> Result<array<ArchiveEntry>, Error>
        archive_entries(archive_read("tar", data, 268435456))
    end

    --" Like `read`, with the largest total size you accept, in bytes. --"
    public fun read_max(data: bytes, max: int) -> Result<array<ArchiveEntry>, Error>
        archive_entries(archive_read("tar", data, max))
    end

    --" A tar archive of these files. --"
    public fun write(entries: array<ArchiveEntry>) -> Result<bytes, Error>
        archive_write("tar", archive_parts(entries))
    end

    --" A gzipped tar archive (`.tar.gz`) of these files. --"
    public fun write_gz(entries: array<ArchiveEntry>) -> Result<bytes, Error>
        archive_write("tar.gz", archive_parts(entries))
    end
end

--" A time zone of the IANA database (`Europe/Paris`, `America/New_York`, `UTC`), with its summer
time rules. A `datetime` is always an instant in UTC; a `Zone` tells what a clock on the wall of that
place reads at that instant, and the other way round. Needs the `qlang` command.

```
let paris = Zone.of("Europe/Paris").unwrap()
let t = datetime.of(2026, 7, 1, 12, 0, 0)               -- 12:00 UTC
print(paris.format(t, "YYYY-MM-DD HH:mm"))             -- 2026-07-01 14:00
print(paris.abbreviation(t))                           -- CEST
let opening = paris.local(2026, 10, 11, 9, 0, 0)       -- 9:00 in Paris, as an instant
```

An hour that does not exist (clocks go forward) is read with the offset before the change; an
hour that happens twice (clocks go back) is the first one. --"
struct Zone
    --" The name of the time zone (`Europe/Paris`). --"
    public name: string
end

impl Zone
    --" The zone called `name`, or `err` for a name that is not in the database. --"
    public fun of(name: string) -> Result<Zone, Error>
        let probe = tz_offset(name, 0)
        if probe.is_err() then
            return Result<Zone, Error>.Err(probe.unwrap_err())
        end
        Result<Zone, Error>.Ok(Zone { name: name })
    end

    --" The offset from UTC at this instant, in seconds (7200 for Paris in summer). --"
    public fun offset_seconds(self, at: datetime) -> int
        int.parse(tz_offset(self.name, at.ms()).unwrap()[0]) ?? 0
    end

    --" The abbreviation at this instant (`CET`, `CEST`, `EST`...; a number such as `+03` for zones
    without one). --"
    public fun abbreviation(self, at: datetime) -> string
        tz_offset(self.name, at.ms()).unwrap()[1]
    end

    --" What a clock on the wall reads at this instant, as a `datetime` whose fields are the wall
    clock (`wall(t).hour()` is the local hour). It is not the instant itself: do not compare it
    with instants. --"
    public fun wall(self, at: datetime) -> datetime
        datetime.at(at.ms() + self.offset_seconds(at) * 1000)
    end

    --" The instant at which a clock in this zone reads this date and time. --"
    public fun local(self, year: int, month: int, day: int, hour: int, minute: int, second: int) -> datetime
        datetime.at(tz_resolve(self.name, datetime.of(year, month, day, hour, minute, second).ms()).unwrap())
    end

    --" `at` written with the clock of this zone, with the patterns of `datetime.format`. --"
    public fun format(self, at: datetime, pattern: string) -> string
        self.wall(at).format(pattern)
    end
end

-- The `qlang` command provides the terminal: these are the `extern fun` it implements.
extern fun term_is_tty(which: int) -> bool
extern fun term_size() -> array<int>
extern fun term_colors() -> int
extern fun term_raw(on: bool) -> Result<bool, Error>
extern fun term_mode(name: string, on: bool) -> Result<bool, Error>
extern fun term_write(text: string) -> Result<bool, Error>
extern fun term_event(timeout_ms: int) -> Result<array<string>, Error>
extern fun term_script(events: array<array<string>>, cols: int, rows: int) -> Result<bool, Error>
extern fun term_screen() -> Result<string, Error>
extern fun term_script_end() -> bool

--" The size of the terminal, in characters. --"
struct Size
    --" The number of columns. --"
    public cols: int = 80
    --" The number of rows. --"
    public rows: int = 24
end

--" Something that happened at the terminal, read by `Term.event`. `kind` says what:

- `"key"`: `key` is the character (`"a"`, `"A"`, `" "`) or the name of the key (`"Enter"`, `"Tab"`,
  `"Esc"`, `"Backspace"`, `"Delete"`, `"Insert"`, `"Up"`, `"Down"`, `"Left"`, `"Right"`, `"Home"`,
  `"End"`, `"PageUp"`, `"PageDown"`, `"F1"`... `"F12"`), with `ctrl`, `alt` and `shift`;
- `"mouse"`: `action` is `"down"`, `"up"`, `"drag"`, `"move"`, `"scroll_up"`, `"scroll_down"`,
  `"scroll_left"` or `"scroll_right"`, `button` is `"left"`, `"right"`, `"middle"` or `"none"`, and `x`
  and `y` are the column and the row (from 0);
- `"resize"`: `x` and `y` are the new number of columns and rows;
- `"paste"`: `text` is what was pasted (after `Term.paste(true)`);
- `"focus"`: `text` is `"in"` or `"out"` (after `Term.focus(true)`);
- `"timeout"`: nothing happened within the time asked. --"
struct Event
    --" `"key"`, `"mouse"`, `"resize"`, `"paste"`, `"focus"` or `"timeout"`. --"
    public kind: string = "timeout"
    --" The character (`"a"`) or the name of the key (`"Enter"`, `"Up"`, `"F1"`…) of a key event. --"
    public key: string = ""
    --" What was pasted (`"paste"`), or `"in"` / `"out"` (`"focus"`). --"
    public text: string = ""
    --" `true` when Ctrl was held. --"
    public ctrl: bool = false
    --" `true` when Alt was held. --"
    public alt: bool = false
    --" `true` when Shift was held. --"
    public shift: bool = false
    --" The column of a mouse event, or the number of columns after a resize (from 0). --"
    public x: int = 0
    --" The row of a mouse event, or the number of rows after a resize (from 0). --"
    public y: int = 0
    --" `"left"`, `"right"`, `"middle"` or `"none"`, for a mouse event. --"
    public button: string = "none"
    --" `"down"`, `"up"`, `"drag"`, `"move"` or `"scroll_up"`, `"scroll_down"`, `"scroll_left"`, `"scroll_right"`, for a mouse event. --"
    public action: string = ""
end

fun term_parse_event(parts: array<string>) -> Event
    let kind = parts[0]
    if kind == "key" then
        let mods = parts[2]
        return Event { kind: "key", key: parts[1], ctrl: mods.contains("ctrl"), alt: mods.contains("alt"), shift: mods.contains("shift") }
    end
    if kind == "mouse" then
        let mods = parts[5]
        return Event { kind: "mouse", action: parts[1], button: parts[2], x: int.parse(parts[3]) ?? 0, y: int.parse(parts[4]) ?? 0, ctrl: mods.contains("ctrl"), alt: mods.contains("alt"), shift: mods.contains("shift") }
    end
    if kind == "resize" then
        return Event { kind: "resize", x: int.parse(parts[1]) ?? 0, y: int.parse(parts[2]) ?? 0 }
    end
    if kind == "paste" or kind == "focus" then
        return Event { kind: kind, text: parts[1] }
    end
    Event { kind: "timeout" }
end

-- an event as the array of texts the host exchanges (the inverse of `term_parse_event`)
fun term_unparse_event(e: Event) -> array<string>
    let mods: array<string> = []
    if e.ctrl then
        mods.push("ctrl")
    end
    if e.alt then
        mods.push("alt")
    end
    if e.shift then
        mods.push("shift")
    end
    let m = mods.join("+")
    if e.kind == "key" then
        return ["key", e.key, m]
    end
    if e.kind == "mouse" then
        return ["mouse", e.action, e.button, "{e.x}", "{e.y}", m]
    end
    if e.kind == "resize" then
        return ["resize", "{e.x}", "{e.y}"]
    end
    if e.kind == "paste" or e.kind == "focus" then
        return [e.kind, e.text]
    end
    ["timeout"]
end

fun term_enter() -> Result<bool, Error>
    let raw = term_raw(true)
    if raw.is_err() then
        return raw
    end
    let alt = term_mode("alt", true)
    if alt.is_err() then
        term_raw(false)
        return alt
    end
    term_mode("hide_cursor", true)
end

fun term_leave()
    term_mode("mouse", false)
    term_mode("paste", false)
    term_mode("focus", false)
    term_mode("hide_cursor", false)
    term_mode("alt", false)
    term_raw(false)
end

--" The terminal, for programs that draw a screen. Needs the `qlang` command, run from a terminal
(`Term.is_tty()`); the size and the number of colours are answered anywhere (80 by 24, no colour).

```
Term.session(fun() -> bool
    Term.write("\e[1;1HHello, press q")
    while true do
        let event = Term.event(250).unwrap()
        if event.kind == "key" and event.key == "q" then
            break
        end
    end
    true
end)
```

Writing is done in one piece per `Term.write` (build the whole screen as a string, write it once).
`Term.session` puts the terminal back as it was when the function ends, and the `qlang` command does
it too if the program fails or stops in the middle. --"
struct Term
end

impl Term
    --" Whether the input and the output are a terminal (not a pipe or a file). --"
    public fun is_tty() -> bool
        term_is_tty(0) and term_is_tty(1)
    end

    --" The size of the terminal (the `COLUMNS` and `LINES` variables, else 80 by 24, when it is not one). --"
    public fun size() -> Size
        let s = term_size()
        Size { cols: s[0], rows: s[1] }
    end

    --" How many colours the terminal shows: 0 (none: not a terminal, `NO_COLOR`), 16, 256 or 16777216
    (true colour). --"
    public fun colors() -> int
        term_colors()
    end

    --" Raw mode: the keys reach the program as they are pressed, without being shown (`false` goes
    back to the usual line mode). --"
    public fun raw(on: bool) -> Result<bool, Error>
        term_raw(on)
    end

    --" Writes text as it is, without a newline, and shows it at once. --"
    public fun write(text: string) -> Result<bool, Error>
        term_write(text)
    end

    --" Writes a whole screen so that the terminal shows it in one piece (no flicker). --"
    public fun frame(text: string) -> Result<bool, Error>
        term_write("\e[?2026h" + text + "\e[?2026l")
    end

    --" Erases the screen and puts the cursor at the top left. --"
    public fun clear() -> Result<bool, Error>
        term_write("\e[2J\e[H")
    end

    --" Sets the title of the window. --"
    public fun title(text: string) -> Result<bool, Error>
        term_write("\e]0;" + text + "\e\\")
    end

    --" The alternate screen: what is drawn there disappears when it is left, and the terminal shows
    again what it showed before. --"
    public fun alt_screen(on: bool) -> Result<bool, Error>
        term_mode("alt", on)
    end

    --" Hides the cursor (`false` shows it). --"
    public fun hide_cursor(on: bool) -> Result<bool, Error>
        term_mode("hide_cursor", on)
    end

    --" Reports the mouse as `"mouse"` events. --"
    public fun mouse(on: bool) -> Result<bool, Error>
        term_mode("mouse", on)
    end

    --" Reports what is pasted as a `"paste"` event, instead of as keys. --"
    public fun paste(on: bool) -> Result<bool, Error>
        term_mode("paste", on)
    end

    --" Reports when the window gains or loses the focus, as `"focus"` events. --"
    public fun focus(on: bool) -> Result<bool, Error>
        term_mode("focus", on)
    end

    --" The next event, waiting at most `timeout_ms` milliseconds (`0` does not wait, a negative number
    waits for ever). An event of kind `"timeout"` says that nothing happened. --"
    public fun event(timeout_ms: int) -> Result<Event, Error>
        let found = term_event(timeout_ms)
        if found.is_err() then
            return Result<Event, Error>.Err(found.unwrap_err())
        end
        Result<Event, Error>.Ok(term_parse_event(found.unwrap()))
    end

    --" A **scripted terminal** for tests (`qlang test`, run with the `qlang` command): from now on the program
    thinks it is in a terminal of `cols` by `rows` where the keys, mouse movements and pastes of `events` arrive
    one after the other (`Event { kind: "key", key: "q" }`); when they are used up, `Term.event` answers
    `"timeout"` at once, never waits. Nothing reaches the real terminal: what the program writes is kept, and
    `Term.screen()` gives it. `Term.end_script()` goes back to the real terminal. --"
    public fun script(events: array<Event>, cols: int, rows: int) -> Result<bool, Error>
        let parts: array<array<string>> = []
        for e in events do
            parts.push(term_unparse_event(e))
        end
        term_script(parts, cols, rows)
    end

    --" Everything the program wrote since `Term.script`, escape sequences included. --"
    public fun screen() -> Result<string, Error>
        term_screen()
    end

    --" Ends the scripted terminal. Gives whether there was one. --"
    public fun end_script() -> bool
        term_script_end()
    end

    --" Runs `body` with the terminal ready for a full screen: raw mode, the alternate screen and no
    cursor. Whatever happens, the terminal is put back as it was when it ends. Its result is the one
    of `body`. --"
    public fun session<T>(body: fun() -> T) -> Result<T, Error>
        let entered = term_enter()
        if entered.is_err() then
            return Result<T, Error>.Err(entered.unwrap_err())
        end
        let result = body()
        term_leave()
        Result<T, Error>.Ok(result)
    end
end

-- The `qlang` command provides Redis: these are the `extern fun` it implements.
extern fun redis_connect(url: string) -> Result<int, Error>
extern fun redis_command(conn: int, args: array<string>) -> Result<Json, Error>
extern fun redis_poll(conn: int, timeout_ms: int) -> Result<Json, Error>
extern fun redis_close(conn: int) -> Result<bool, Error>

--" A message received on a Redis subscription. --"
struct RedisMessage
    --" The channel the message came from. --"
    public channel: string
    --" The message. --"
    public payload: string
    --" The pattern that matched, for a `psubscribe`. --"
    public pattern: string? = none
end

--" A Redis connection, opened with `Redis.connect(url)`: `redis://[user:password@]host[:port][/db]`, or
`rediss://` for TLS. A server that is not on this machine needs `rediss://` (or `?tls=disable` to say
that you want it in clear). Every command waits for its answer (30 s at most), so a connection serves
one thing at a time: use one connection for commands and another for a subscription. --"
struct Redis
    --" The number of the connection, for the host. --"
    public conn: int
end

impl Redis
    --" Opens a connection. Needs the `qlang` command. --"
    public fun connect(url: string) -> Result<Redis, Error>
        let r = redis_connect(url)
        if r.is_ok() then
            Result.Ok(Redis { conn: r.unwrap() })
        else
            Result.Err(r.unwrap_err())
        end
    end

    --" Opens a connection that only listens: it is subscribed to these channels, and `next` reads what
    is published. --"
    public fun subscriber(url: string, channels: array<string>) -> Result<Subscription, Error>
        let r = redis_connect(url)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let sub = Subscription { conn: r.unwrap() }
        if channels.is_empty() then
            return Result.Ok(sub)
        end
        let done = sub.subscribe(channels)
        if done.is_err() then
            sub.close()
            return Result.Err(done.unwrap_err())
        end
        Result.Ok(sub)
    end

    --" Any command, as its words: `r.command(["SET", "k", "v"])`. The answer is a `Json`: a text, a
    number, `null`, or an array of those. A Redis error is an `err`. --"
    public fun command(self, args: array<string>) -> Result<Json, Error>
        redis_command(self.conn, args)
    end

    fun int_of(r: Result<Json, Error>) -> Result<int, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_number() then
            Result.Ok(j.as_number() as int)
        else
            Result.Err(Error { message: "Redis: a number was expected" })
        end
    end

    fun text_of(r: Result<Json, Error>) -> Result<string?, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_string() then
            Result<string?, Error>.Ok(j.as_string())
        else
            Result<string?, Error>.Ok(none)
        end
    end

    fun texts_of(r: Result<Json, Error>) -> Result<array<string>, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        let out: array<string> = []
        if j.is_array() then
            for x in j.as_array() do
                if x.is_string() then
                    out.push(x.as_string())
                end
            end
        end
        Result.Ok(out)
    end

    fun ok_of(r: Result<Json, Error>) -> Result<bool, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        Result.Ok(j.is_string() and j.as_string() == "OK")
    end

    --" The value of a key, or `none`. --"
    public fun get(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["GET", key]))
    end

    --" Set the string `key` to `value`. --"
    public fun set(self, key: string, value: string) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value]))
    end

    --" Sets a value that expires after `seconds`. --"
    public fun set_ex(self, key: string, value: string, seconds: int) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value, "EX", "{seconds}"]))
    end

    --" Sets the value only if the key does not exist (a lock): `true` when it was set. It expires
    after `seconds`. --"
    public fun set_if_absent(self, key: string, value: string, seconds: int) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value, "NX", "EX", "{seconds}"]))
    end

    --" Deletes a key; how many existed (0 or 1). --"
    public fun del(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["DEL", key]))
    end

    --" `true` when `key` exists. --"
    public fun exists(self, key: string) -> Result<bool, Error>
        let n = Redis.int_of(self.command(["EXISTS", key]))
        if n.is_err() then
            return Result.Err(n.unwrap_err())
        end
        Result.Ok(n.unwrap() > 0)
    end

    --" Adds 1 to a number (0 if the key does not exist); gives the new value. --"
    public fun incr(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["INCR", key]))
    end

    --" The key expires after `seconds`; `false` if it does not exist. --"
    public fun expire(self, key: string, seconds: int) -> Result<bool, Error>
        let n = Redis.int_of(self.command(["EXPIRE", key, "{seconds}"]))
        if n.is_err() then
            return Result.Err(n.unwrap_err())
        end
        Result.Ok(n.unwrap() > 0)
    end

    --" Puts a value at the start of a list; gives the length of the list. --"
    public fun lpush(self, key: string, value: string) -> Result<int, Error>
        Redis.int_of(self.command(["LPUSH", key, value]))
    end

    --" Puts a value at the end of a list; gives the length of the list. --"
    public fun rpush(self, key: string, value: string) -> Result<int, Error>
        Redis.int_of(self.command(["RPUSH", key, value]))
    end

    --" Remove and return the first element of the list `key`, or `none` when it is empty. --"
    public fun lpop(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["LPOP", key]))
    end

    --" Remove and return the last element of the list `key`, or `none` when it is empty. --"
    public fun rpop(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["RPOP", key]))
    end

    --" The length of the list `key`. --"
    public fun llen(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["LLEN", key]))
    end

    --" The elements of the list `key` from `start` to `stop` (inclusive; negative counts from the end). --"
    public fun lrange(self, key: string, start: int, stop: int) -> Result<array<string>, Error>
        Redis.texts_of(self.command(["LRANGE", key, "{start}", "{stop}"]))
    end

    --" Takes the first element of the first non-empty list, waiting up to `seconds` for one (0: for
    ever). Gives (list, element), or `none` when the time ran out. --"
    public fun blpop(self, keys: array<string>, seconds: int) -> Result<(string, string)?, Error>
        let words = ["BLPOP"]
        for k in keys do
            words.push(k)
        end
        words.push("{seconds}")
        let r = self.command(words)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if not j.is_array() then
            return Result.Ok(none)
        end
        let parts = j.as_array()
        if parts.len() != 2 then
            return Result.Ok(none)
        end
        Result<(string, string)?, Error>.Ok((parts[0].as_string(), parts[1].as_string()))
    end

    --" Adds a member to a sorted set with a score (or changes its score); how many were added. --"
    public fun zadd(self, key: string, score: float, member: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZADD", key, "{score}", member]))
    end

    --" Remove `member` from the sorted set `key`; the number removed. --"
    public fun zrem(self, key: string, member: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZREM", key, member]))
    end

    --" The number of members of the sorted set `key`. --"
    public fun zcard(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZCARD", key]))
    end

    --" The members whose score is from `min` to `max`, lowest first, at most `limit`. --"
    public fun zrange_by_score(self, key: string, min: float, max: float, limit: int) -> Result<array<string>, Error>
        Redis.texts_of(self.command(["ZRANGEBYSCORE", key, "{min}", "{max}", "LIMIT", "0", "{limit}"]))
    end

    --" Sends a message to a channel; how many subscribers received it. --"
    public fun publish(self, channel: string, message: string) -> Result<int, Error>
        Redis.int_of(self.command(["PUBLISH", channel, message]))
    end

    --" Runs a Lua script atomically: `KEYS` and `ARGV` are what you give. --"
    public fun eval(self, script: string, keys: array<string>, args: array<string>) -> Result<Json, Error>
        let words = ["EVAL", script, "{keys.len()}"]
        for k in keys do
            words.push(k)
        end
        for a in args do
            words.push(a)
        end
        self.command(words)
    end

    --" Close the connection. --"
    public fun close(self) -> Result<bool, Error>
        redis_close(self.conn)
    end
end

--" A connection that listens to channels (`Redis.subscriber`). --"
struct Subscription
    --" The number of the connection. --"
    public conn: int
end

impl Subscription
    --" Listens to more channels. --"
    public fun subscribe(self, channels: array<string>) -> Result<bool, Error>
        self.send("SUBSCRIBE", channels)
    end

    --" Stop listening to `channels`. --"
    public fun unsubscribe(self, channels: array<string>) -> Result<bool, Error>
        self.send("UNSUBSCRIBE", channels)
    end

    --" Listens to the channels that match patterns (`news.*`). --"
    public fun psubscribe(self, patterns: array<string>) -> Result<bool, Error>
        self.send("PSUBSCRIBE", patterns)
    end

    fun send(self, verb: string, names: array<string>) -> Result<bool, Error>
        let words = [verb]
        for n in names do
            words.push(n)
        end
        let r = redis_command(self.conn, words)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        Result.Ok(true)
    end

    --" The next message, waiting at most `timeout_ms` (0: only what already arrived). `none` when
    nothing came. --"
    public fun next(self, timeout_ms: int) -> Result<RedisMessage?, Error>
        let r = redis_poll(self.conn, timeout_ms)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_null() then
            return Result.Ok(none)
        end
        let channel = j.get("channel")
        let payload = j.get("payload")
        if channel == none or payload == none or not channel.is_string() or not payload.is_string() then
            return Result.Ok(none)
        end
        let pattern = j.get("pattern")
        let p: string? = if pattern != none and pattern.is_string() then pattern.as_string() else none end
        Result<RedisMessage?, Error>.Ok(RedisMessage { channel: channel.as_string(), payload: payload.as_string(), pattern: p })
    end

    --" Close the connection. --"
    public fun close(self) -> Result<bool, Error>
        redis_close(self.conn)
    end
end

--" A SQL database connection, opened with `Db.connect(url)`. --"
struct Db
    conn: int
end

impl Db
    --" Opens a database connection (`sqlite://path`, `postgres://...`). Needs database access. --"
    public fun connect(url: string) -> Result<Db, Error>
        let r = sql_connect(url)
        if r.is_ok() then
            Result.Ok(Db { conn: r.unwrap() })
        else
            Result.Err(r.unwrap_err())
        end
    end

    --" Runs a statement that returns rows (`SELECT`). `?` placeholders are bound from `params`, in order. --"
    public fun query(self, sql: string, params: array<Json>) -> Result<array<map<string, Json>>, Error>
        sql_query(self.conn, sql, params)
    end

    --" Runs a statement that does not return rows (`INSERT`/`UPDATE`/`DELETE`/DDL), returning the number of rows affected. --"
    public fun execute(self, sql: string, params: array<Json>) -> Result<int, Error>
        sql_execute(self.conn, sql, params)
    end

    --" Closes the connection. --"
    public fun close(self) -> Result<bool, Error>
        sql_close(self.conn)
    end

    --" Starts a transaction (a savepoint when one is already open: `begin` can be nested, and each `commit` or
    `rollback` closes the last `begin`). --"
    public fun begin(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "BEGIN", []))
    end

    --" Commits the transaction started by `begin()`. --"
    public fun commit(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "COMMIT", []))
    end

    --" Cancels everything done since `begin()`. --"
    public fun rollback(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "ROLLBACK", []))
    end

    --" Runs `body` in a transaction: the transaction is committed when it gives `ok`, and rolled back
    when it gives `err` (which `transaction` gives back). Inside another transaction it is a savepoint:
    only what `body` did is undone. --"
    public fun transaction<T>(self, body: fun() -> Result<T, Error>) -> Result<T, Error>
        let started = sql_done(sql_execute(self.conn, "BEGIN", []))
        if started.is_err() then
            return Result<T, Error>.Err(started.unwrap_err())
        end
        let outcome = body()
        if outcome.is_err() then
            sql_execute(self.conn, "ROLLBACK", [])
            return outcome
        end
        let done = sql_done(sql_execute(self.conn, "COMMIT", []))
        if done.is_err() then
            return Result<T, Error>.Err(done.unwrap_err())
        end
        outcome
    end
end
"#,
    include_str!("prelude_big.q")
);

fn compile_inner(
    entry: &str,
    source: &str,
    host: &mut dyn Host,
    options: &CompileOptions,
) -> Result<Program, CompileError> {
    let caps = host.capabilities();
    let mut loader = Loader {
        host,
        sources: SourceMap::default(),
        diags: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        done: HashMap::new(),
        next_id: 0,
    };
    loader.discover(
        PRELUDE_PATH,
        PRELUDE_PATH.to_string(),
        PRELUDE_SOURCE.to_string(),
    );
    let entry_key = loader.host.canonical_path(entry);
    loader.discover(entry, entry_key, source.to_string());
    let (sources, mut diags, modules) = loader.finish();

    let has_errors = |d: &[Diagnostic]| d.iter().any(|x| x.severity == Severity::Error);
    if has_errors(&diags) {
        return Err(finish_error(diags, sources, None));
    }

    let missing_externs: HashSet<String> = modules
        .iter()
        .flat_map(|m| m.ast.items.iter())
        .filter_map(|item| match item {
            Item::Fun(f) if f.external => f.name.as_ref().map(|n| n.name.clone()),
            _ => None,
        })
        .filter(|name| !host.has_extern(name))
        .collect();
    let mut ck = Checker::new(sources, modules, caps);
    ck.missing_externs = missing_externs;
    if options.info {
        ck.collect = Some(info::Collector::default());
    }
    ck.run();
    diags.append(&mut ck.diags);
    let info = if options.info { ck.build_info() } else { None };
    if has_errors(&diags) {
        return Err(finish_error(diags, ck.sources, info));
    }
    for d in &mut diags {
        d.resolve(&ck.sources);
    }
    Ok(Program {
        sources: ck.sources,
        modules: ck
            .modules
            .into_iter()
            .map(|m| ModuleData {
                path: m.path,
                file: m.file,
                ast: m.ast,
            })
            .collect(),
        defs: ck.defs,
        res: ck.res,
        warnings: diags,
        info,
    })
}

fn finish_error(
    mut diags: Vec<Diagnostic>,
    sources: SourceMap,
    info: Option<Info>,
) -> CompileError {
    // stable order: by file then position
    diags.sort_by_key(|d| (d.span.file, d.span.start));
    if diags.len() > MAX_DIAGNOSTICS {
        let last = diags[MAX_DIAGNOSTICS - 1].span;
        diags.truncate(MAX_DIAGNOSTICS);
        diags.push(Diagnostic::error(
            "T000",
            format!("too many errors: only the first {MAX_DIAGNOSTICS} are shown"),
            last,
        ));
    }
    for d in &mut diags {
        d.resolve(&sources);
    }
    CompileError {
        diagnostics: diags,
        sources,
        info: info.map(Box::new),
    }
}

// ------------------------------------------------------------------ loading

pub(crate) struct Loaded {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    /// Item index of each `import` to the index of the module it loads.
    imports: HashMap<usize, usize>,
}

/// A module as discovered, before modules are put in execution order. `imports` and
/// `done`/edges below refer to other modules by *discovery* id, not by final [`ModId`]:
/// those only agree once [`Loader::finish`] has decided an order.
struct Discovered {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    imports: HashMap<usize, usize>,
    /// Whether this module has any top-level statement (as opposed to only
    /// declarations), i.e. whether *when* it runs relative to other modules matters.
    runs_code: bool,
}

struct Loader<'h> {
    host: &'h mut dyn Host,
    sources: SourceMap,
    diags: Vec<Diagnostic>,
    nodes: Vec<Discovered>,
    /// Every `import` edge seen, as (importer, imported, span of the import path) — kept
    /// only to point at a concrete import if a cycle turns out to have no valid order.
    edges: Vec<(usize, usize, Span)>,
    done: HashMap<String, usize>,
    /// Node ids are unique across all modules.
    next_id: u32,
}

impl Loader<'_> {
    /// `key` identifies the module (see `Host::canonical_path`); `path` is how it is named.
    /// Discovers every module reachable from this one. A module's discovery id is
    /// reserved (in `done`) *before* its own imports are followed, so an import cycle
    /// (`a.q` and `b.q` importing each other) simply links back to the id already
    /// reserved instead of recursing forever or needing to be rejected here.
    fn discover(&mut self, path: &str, key: String, text: String) -> usize {
        let file = self.sources.add(path, &text);
        let (ast, next_id) = parse_module_from(&text, file, self.next_id, &mut self.diags);
        self.next_id = next_id;
        let runs_code = ast.items.iter().any(|it| matches!(it, Item::Stmt(_)));
        let id = self.nodes.len();
        self.nodes.push(Discovered {
            path: path.to_string(),
            file,
            ast: Rc::new(ast),
            imports: HashMap::new(),
            runs_code,
        });
        self.done.insert(key, id);

        let ast = self.nodes[id].ast.clone();
        let mut imports = HashMap::new();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(target) = resolve_module_path(path, &imp.path) else {
                crate::diag::push_capped(
                    &mut self.diags,
                    Diagnostic::error(
                        "T100",
                        format!(
                            "cannot import \"{}\": use a path relative to this file, written with `/` (`./x.q`, `../lib/x.q`) or a package (`@name/x.q`), not an absolute one",
                            imp.path
                        ),
                        imp.path_span,
                    ),
                );
                continue;
            };
            let target = if crate::span::is_package_path(&target) {
                self.host.package_file(&target)
            } else {
                target
            };
            let target_key = self.host.canonical_path(&target);
            let m = if let Some(&m) = self.done.get(&target_key) {
                m
            } else {
                match self.host.load_module(&target) {
                    Ok(src) => self.discover(&target, target_key, src),
                    Err(e) => {
                        crate::diag::push_capped(
                            &mut self.diags,
                            Diagnostic::error(
                                "T102",
                                format!("cannot import \"{}\": {e}", imp.path),
                                imp.path_span,
                            ),
                        );
                        continue;
                    }
                }
            };
            imports.insert(idx, m);
            self.edges.push((id, m, imp.path_span));
        }
        self.nodes[id].imports = imports;
        id
    }

    /// Put the discovered modules in a valid execution order (a module after the
    /// modules it imports), rejecting only the cycles for which no such order exists:
    /// those where more than one module actually runs top-level code. Modules that
    /// only declare things (structs, fns, traits...) can be part of a cycle freely,
    /// since nothing about them depends on *when* they run.
    fn order(&mut self) -> Vec<usize> {
        let n = self.nodes.len();
        let succ: Vec<Vec<usize>> = (0..n)
            .map(|i| {
                let mut v: Vec<usize> = self.nodes[i].imports.values().copied().collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (u, outs) in succ.iter().enumerate() {
            for &v in outs {
                pred[v].push(u);
            }
        }

        // Kosaraju, step 1: finishing order of a DFS over the import graph.
        let mut visited = vec![false; n];
        let mut finish: Vec<usize> = Vec::with_capacity(n);
        for start in 0..n {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut stack = vec![(start, 0usize)];
            while let Some(&mut (u, ref mut i)) = stack.last_mut() {
                if *i < succ[u].len() {
                    let v = succ[u][*i];
                    *i += 1;
                    if !visited[v] {
                        visited[v] = true;
                        stack.push((v, 0));
                    }
                } else {
                    finish.push(u);
                    stack.pop();
                }
            }
        }

        // Step 2: strongly connected components, found on the reverse graph in
        // decreasing finishing time. This yields the SCCs in topological order of the
        // import graph (an importer's SCC before the SCCs it imports); reversed, that
        // becomes "a module's SCC comes after the SCCs it imports", which is what
        // execution order needs.
        let mut visited = vec![false; n];
        let mut sccs: Vec<Vec<usize>> = Vec::new();
        for &start in finish.iter().rev() {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut comp = vec![start];
            let mut stack = vec![start];
            while let Some(u) = stack.pop() {
                for &v in &pred[u] {
                    if !visited[v] {
                        visited[v] = true;
                        comp.push(v);
                        stack.push(v);
                    }
                }
            }
            comp.sort_unstable();
            sccs.push(comp);
        }
        sccs.reverse();

        for comp in &sccs {
            if comp.len() < 2 {
                continue;
            }
            let runs: Vec<usize> = comp
                .iter()
                .copied()
                .filter(|&m| self.nodes[m].runs_code)
                .collect();
            if runs.len() > 1 {
                let span = self
                    .edges
                    .iter()
                    .find(|&&(u, v, _)| comp.contains(&u) && comp.contains(&v))
                    .map(|&(_, _, s)| s)
                    .unwrap_or_default();
                let names: Vec<&str> = runs.iter().map(|&m| self.nodes[m].path.as_str()).collect();
                crate::diag::push_capped(
                    &mut self.diags,
                    Diagnostic::error(
                        "T101",
                        format!(
                            "these modules import each other in a cycle and each runs top-level code, so there is no valid order to run them: {}",
                            names.join(", ")
                        ),
                        span,
                    ),
                );
            }
        }

        sccs.into_iter().flatten().collect()
    }

    /// Consume the loader, deciding final module order and remapping every `imports`
    /// map from discovery ids to the resulting [`ModId`]s.
    fn finish(mut self) -> (SourceMap, Vec<Diagnostic>, Vec<Loaded>) {
        let order = self.order();
        let mut pos = vec![0usize; self.nodes.len()];
        for (new_id, &old_id) in order.iter().enumerate() {
            pos[old_id] = new_id;
        }
        let mut modules: Vec<Option<Loaded>> = (0..self.nodes.len()).map(|_| None).collect();
        for (old_id, node) in self.nodes.into_iter().enumerate() {
            let imports = node
                .imports
                .iter()
                .map(|(&idx, &target)| (idx, pos[target]))
                .collect();
            modules[pos[old_id]] = Some(Loaded {
                path: node.path,
                file: node.file,
                ast: node.ast,
                imports,
            });
        }
        let modules = modules
            .into_iter()
            .map(|m| m.expect("every position was assigned"))
            .collect();
        (self.sources, self.diags, modules)
    }
}

// ------------------------------------------------------------------ checker

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ItemRef {
    Fn(FnId),
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
    Alias(usize),
    Var(ModId, String),
    Module(ModId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalKind {
    Var,
    Const,
    Param,
}

#[derive(Clone, Debug)]
pub(crate) struct Local {
    pub ty: Ty,
    pub kind: LocalKind,
    /// Set when `ty` is a narrowed view (`x != none`) of this declared type.
    pub orig: Option<Ty>,
    /// A narrowed view, kept in a local scope, of a variable of the module.
    pub from_module: bool,
    pub span: Span,
}

#[derive(Default)]
pub(crate) struct ModScope {
    pub items: HashMap<String, ItemRef>,
    pub exports: HashSet<String>,
    /// Top-level variables, by name.
    pub vars: HashMap<String, Local>,
    /// Item index in the file to the id of its definition.
    pub decl_ids: HashMap<usize, usize>,
    /// Variables some function or lambda of this file assigns (never narrowed).
    pub assigned_in_closures: HashSet<String>,
}

pub(crate) struct Checker {
    pub caps: Capabilities,
    /// Names of the `extern fun` this host does not provide.
    pub missing_externs: HashSet<String>,
    pub sources: SourceMap,
    pub diags: Vec<Diagnostic>,
    pub defs: Defs,
    pub res: Resolutions,
    pub modules: Vec<Loaded>,
    pub mscopes: Vec<ModScope>,

    // --- state of the body being checked
    pub cur_mod: ModId,
    pub scopes: Vec<HashMap<String, Local>>,
    pub tparam_scope: Vec<(String, ParamId)>,
    /// What `Self` means here: the type an `impl` is for, or the (still unknown) implementer in a trait.
    pub self_ty: Option<Ty>,
    /// Return type of the enclosing function or lambda.
    pub ret_stack: Vec<Ty>,
    pub loop_depth: usize,
    /// The struct whose `impl` is being checked (for visibility).
    pub cur_impl_struct: Option<StructId>,
    /// The type of `self` in the current method.
    pub cur_self: Option<Ty>,
    /// The method being checked (for `super`).
    pub cur_fn: Option<FnId>,
    /// The function the next `check_args` call is about (to record its type arguments).
    pub callee_hint: Option<FnId>,
    pub subst: Vec<Option<Ty>>,
    /// Set when an editor asked for [`Info`].
    pub collect: Option<info::Collector>,
    /// The documentation comments (filled when building [`Info`]).
    pub(crate) docs: info::Docs,
}

impl Checker {
    fn new(sources: SourceMap, modules: Vec<Loaded>, caps: Capabilities) -> Checker {
        let n = modules.len();
        Checker {
            caps,
            missing_externs: HashSet::new(),
            sources,
            diags: Vec::new(),
            defs: Defs::new(),
            res: Resolutions::default(),
            modules,
            mscopes: (0..n).map(|_| ModScope::default()).collect(),
            cur_mod: 0,
            scopes: Vec::new(),
            tparam_scope: Vec::new(),
            self_ty: None,
            ret_stack: Vec::new(),
            loop_depth: 0,
            cur_impl_struct: None,
            cur_self: None,
            cur_fn: None,
            callee_hint: None,
            subst: Vec::new(),
            collect: None,
            docs: info::Docs::default(),
        }
    }

    pub fn err(&mut self, code: &str, msg: impl Into<String>, span: Span) {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span));
    }

    pub fn err_note(
        &mut self,
        code: &str,
        msg: impl Into<String>,
        span: Span,
        note: impl Into<String>,
        note_span: Option<Span>,
    ) {
        crate::diag::push_capped(
            &mut self.diags,
            Diagnostic::error(code, msg, span).with_note(note, note_span),
        );
    }

    pub fn show(&self, t: &Ty) -> String {
        let t = self.resolve(t);
        t.show(&self.defs)
    }

    fn run(&mut self) {
        let n = self.modules.len();
        // A: declare names
        for m in 0..n {
            self.cur_mod = m;
            self.declare_items(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.declare_own_exports(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_imports(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.declare_reexports(m);
        }
        // B: resolve signatures
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_traits(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_structs(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_enums(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_functions(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_impls(m);
        }
        // an alias nobody uses is still checked
        for id in 0..self.defs.aliases.len() {
            let a = &self.defs.aliases[id];
            let (path, span) = (
                vec![crate::ast::Ident {
                    name: a.name.clone(),
                    span: a.span,
                }],
                a.span,
            );
            self.cur_mod = self.defs.aliases[id].module;
            self.item_to_type(ItemRef::Alias(id), &path, &[], span);
        }
        self.validate_structs();
        self.validate_overrides();
        if self.diags.iter().any(|d| d.severity == Severity::Error) {
            return;
        }
        // C: bodies
        for m in 0..n {
            self.cur_mod = m;
            self.check_module_body(m);
        }
        self.finish_expr_tys();
    }
}
