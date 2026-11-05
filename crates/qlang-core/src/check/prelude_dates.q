
-- ===== dates without a time, times without a date, instants with a zone =====

fun date_leap(year: int) -> bool
    (year % 4 == 0 and year % 100 != 0) or year % 400 == 0
end

fun date_month_days(year: int, month: int) -> int
    if month == 2 then
        return if date_leap(year) then 29 else 28 end
    end
    if month == 4 or month == 6 or month == 9 or month == 11 then 30 else 31 end
end

--" A day of the calendar, with no time and no zone: a birthday, a holiday, a due date. `Date.of(2026, 10, 11)` is
`none` for a day that does not exist. A `Date` is the same everywhere (no UTC hidden behind it); `to_datetime()` gives
midnight UTC when an instant is needed. Compare with `== < >`, and move with `plus_days`, `plus_months`, `plus_years`. --"
struct Date
    --" The year, 1 to 9999. --"
    public year: int
    --" The month, 1 to 12. --"
    public month: int
    --" The day of the month, 1 to 31. --"
    public day: int
end

impl Date
    --" The day `year`-`month`-`day`, or `none` if it does not exist. --"
    public fun of(year: int, month: int, day: int) -> Date?
        if year < 1 or year > 9999 or month < 1 or month > 12 or day < 1 or day > date_month_days(year, month) then
            return none
        end
        Date { year: year, month: month, day: day }
    end

    --" The day of a `datetime` (in UTC). --"
    public fun of_datetime(at: datetime) -> Date
        Date { year: at.year(), month: at.month(), day: at.day() }
    end

    --" Reads `2026-10-11`; `none` for any other text or a day that does not exist. --"
    public fun parse(text: string) -> Date?
        let at = datetime.parse(text)
        if at == none or text.len() != 10 then
            return none
        end
        Date.of_datetime(at)
    end

    --" Midnight UTC of this day. --"
    public fun to_datetime(self) -> datetime
        datetime.of(self.year, self.month, self.day)
    end

    --" `2026-10-11`. --"
    public fun to_iso(self) -> string
        self.to_datetime().format("YYYY-MM-DD")
    end

    --" The day written with the tokens of `datetime.format` (`DD/MM/YYYY`). --"
    public fun format(self, pattern: string) -> string
        self.to_datetime().format(pattern)
    end

    --" Like `format`, with the names in a language (`"fr"`...). --"
    public fun format_in(self, pattern: string, locale: string) -> string
        self.to_datetime().format_in(pattern, locale)
    end

    --" The day after adding `n` days (negative to go back). --"
    public fun plus_days(self, n: int) -> Date
        Date.of_datetime(self.to_datetime().add_days(n))
    end

    --" The day after adding `n` months; 31 January + 1 month is the last day of February. --"
    public fun plus_months(self, n: int) -> Date
        Date.of_datetime(self.to_datetime().add_months(n))
    end

    --" The day after adding `n` years. --"
    public fun plus_years(self, n: int) -> Date
        Date.of_datetime(self.to_datetime().add_years(n))
    end

    --" 1 (Monday) to 7 (Sunday). --"
    public fun weekday(self) -> int
        self.to_datetime().weekday()
    end

    --" 1 January is day 1. --"
    public fun day_of_year(self) -> int
        self.to_datetime().day_of_year()
    end

    --" Whether the year has 366 days. --"
    public fun is_leap_year(self) -> bool
        date_leap(self.year)
    end

    --" 28 to 31. --"
    public fun days_in_month(self) -> int
        date_month_days(self.year, self.month)
    end

    --" Whole days from this day to `other` (negative if `other` is earlier). --"
    public fun days_until(self, other: Date) -> int
        self.to_datetime().diff_in_days(other.to_datetime())
    end

    --" The days from this one to `last` (included), one `gap` apart. --"
    public fun each(self, gap: Duration, until: Date) -> DatePeriod
        self.to_datetime().each(gap, until.to_datetime())
    end
end

impl Eq for Date
    --" Whether both are the same day (`==`). --"
    public fun eq(self, other: Date) -> bool
        self.year == other.year and self.month == other.month and self.day == other.day
    end
end

impl Ord for Date
    --" -1, 0 or 1 (`<`, `>`). --"
    public fun cmp(self, other: Date) -> int
        if self.year != other.year then
            return if self.year < other.year then -1 else 1 end
        end
        if self.month != other.month then
            return if self.month < other.month then -1 else 1 end
        end
        if self.day != other.day then
            return if self.day < other.day then -1 else 1 end
        end
        0
    end
end

--" A time of day, with no date and no zone: an opening hour, an alarm. `Time.of(9, 30)` is `none` for a time that
does not exist (hour 0 to 23, minute and second 0 to 59). Compare with `== < >`; `plus_minutes` wraps around midnight. --"
struct Time
    --" The hour, 0 to 23. --"
    public hour: int
    --" The minute, 0 to 59. --"
    public minute: int
    --" The second, 0 to 59. --"
    public second: int
    --" The milliseconds, 0 to 999. --"
    public ms: int
end

impl Time
    --" The time `hour:minute:second.ms`, or `none` if it does not exist. --"
    public fun of(hour: int, minute: int, second: int = 0, ms: int = 0) -> Time?
        if hour < 0 or hour > 23 or minute < 0 or minute > 59 or second < 0 or second > 59 or ms < 0 or ms > 999 then
            return none
        end
        Time { hour: hour, minute: minute, second: second, ms: ms }
    end

    --" The time of day of a `datetime` (in UTC). --"
    public fun of_datetime(at: datetime) -> Time
        Time { hour: at.hour(), minute: at.minute(), second: at.second(), ms: at.ms() % 1000 }
    end

    --" Reads `14:30`, `14:30:00` or `14:30:00.250`; `none` for any other text. --"
    public fun parse(text: string) -> Time?
        let at = datetime.parse_with("2000-01-01 " + text, if text.len() == 5 then "YYYY-MM-DD HH:mm" elseif text.len() == 8 then "YYYY-MM-DD HH:mm:ss" else "YYYY-MM-DD HH:mm:ss.SSS" end)
        if at == none then
            return none
        end
        Time.of_datetime(at)
    end

    --" Milliseconds since midnight. --"
    public fun to_ms(self) -> int
        ((self.hour * 60 + self.minute) * 60 + self.second) * 1000 + self.ms
    end

    --" `14:30:00`, or `14:30:00.250` with milliseconds. --"
    public fun to_iso(self) -> string
        let base = "{self.hour}".pad_left(2, "0") + ":" + "{self.minute}".pad_left(2, "0") + ":" + "{self.second}".pad_left(2, "0")
        if self.ms == 0 then base else base + "." + "{self.ms}".pad_left(3, "0") end
    end

    --" The time written with the tokens of `datetime.format` (`HH:mm`, `h:mm A`). --"
    public fun format(self, pattern: string) -> string
        datetime.at(self.to_ms()).format(pattern)
    end

    --" The time `n` minutes later, around the clock (negative to go back). --"
    public fun plus_minutes(self, n: int) -> Time
        let day = 86400000
        let t = (self.to_ms() + n * 60000) % day
        if t < 0 then t += day end
        Time.of_datetime(datetime.at(t))
    end

    --" Whole minutes from this time to `other` on the same day (negative if `other` is earlier). --"
    public fun minutes_until(self, other: Time) -> int
        (other.to_ms() - self.to_ms()) div 60000
    end

    --" The instant at which this time is on `date` (UTC). --"
    public fun on(self, date: Date) -> datetime
        datetime.at(date.to_datetime().ms() + self.to_ms())
    end
end

impl Eq for Time
    --" Whether both are the same time (`==`). --"
    public fun eq(self, other: Time) -> bool
        self.to_ms() == other.to_ms()
    end
end

impl Ord for Time
    --" -1, 0 or 1 (`<`, `>`). --"
    public fun cmp(self, other: Time) -> int
        let a = self.to_ms()
        let b = other.to_ms()
        if a == b then 0 elseif a < b then -1 else 1 end
    end
end

--" An instant and the time zone it is seen in: `datetime.of(2026, 7, 1, 12, 0, 0).in_zone(paris)`. It keeps the instant
(so two `Zoned` compare as instants, whatever their zones) and the zone (so `hour()`, `format` and `plus_days` use the wall
clock of that zone, with its daylight saving time). Needs the `qlang` command, like `Zone`. --"
struct Zoned
    --" The instant. --"
    public instant: datetime
    --" The zone it is seen in. --"
    public zone: Zone
end

impl Zoned
    --" The instant `at` seen in `zone`. --"
    public fun of(at: datetime, zone: Zone) -> Zoned
        Zoned { instant: at, zone: zone }
    end

    --" The same instant seen in another zone. --"
    public fun in_zone(self, zone: Zone) -> Zoned
        Zoned { instant: self.instant, zone: zone }
    end

    --" The wall clock of the zone at this instant, as a `datetime` whose fields are the clock's (not an instant). --"
    public fun wall(self) -> datetime
        self.zone.wall(self.instant)
    end

    --" The hour on the wall clock. --"
    public fun hour(self) -> int
        self.wall().hour()
    end

    --" The offset from UTC in seconds (7200 for Paris in summer). --"
    public fun offset_seconds(self) -> int
        self.zone.offset_seconds(self.instant)
    end

    --" The abbreviation of the zone at this instant (`CEST`). --"
    public fun abbreviation(self) -> string
        self.zone.abbreviation(self.instant)
    end

    --" The day on the wall clock. --"
    public fun date(self) -> Date
        Date.of_datetime(self.wall())
    end

    --" The time of day on the wall clock. --"
    public fun time(self) -> Time
        Time.of_datetime(self.wall())
    end

    --" Written with the clock of the zone, with the tokens of `datetime.format`. --"
    public fun format(self, pattern: string) -> string
        self.zone.format(self.instant, pattern)
    end

    --" `2026-07-01T14:00:00+02:00`. --"
    public fun to_iso(self) -> string
        let off = self.offset_seconds()
        let sign = if off < 0 then "-" else "+" end
        let a = off.abs() div 60
        self.wall().format("YYYY-MM-DD[T]HH:mm:ss") + sign + "{a div 60}".pad_left(2, "0") + ":" + "{a % 60}".pad_left(2, "0")
    end

    --" After adding a duration: years, months and days move the wall clock (so a day is a day across a daylight saving
    change, and the hour stays the same), hours, minutes, seconds and milliseconds move the instant. --"
    public fun plus(self, d: Duration) -> Zoned
        let w = self.wall()
        let calendar = Duration.of(years: d.years, months: d.months, days: d.days)
        let moved = w.plus(calendar)
        let at = self.zone.local(moved.year(), moved.month(), moved.day(), moved.hour(), moved.minute(), moved.second())
        let rest = Duration.of(hours: d.hours, minutes: d.minutes, seconds: d.seconds, ms: d.ms)
        Zoned { instant: at.plus(rest), zone: self.zone }
    end

    --" After taking a duration away. --"
    public fun minus(self, d: Duration) -> Zoned
        self.plus(d.negate())
    end
end

impl Eq for Zoned
    --" Whether both are the same instant, whatever their zones (`==`). --"
    public fun eq(self, other: Zoned) -> bool
        self.instant.ms() == other.instant.ms()
    end
end

impl Ord for Zoned
    --" -1, 0 or 1 by instant (`<`, `>`). --"
    public fun cmp(self, other: Zoned) -> int
        let a = self.instant.ms()
        let b = other.instant.ms()
        if a == b then 0 elseif a < b then -1 else 1 end
    end
end

impl datetime
    --" This instant seen in a zone. --"
    public fun in_zone(self, zone: Zone) -> Zoned
        Zoned { instant: self, zone: zone }
    end

    --" The day of this instant (in UTC). --"
    public fun date(self) -> Date
        Date.of_datetime(self)
    end

    --" The time of day of this instant (in UTC). --"
    public fun time(self) -> Time
        Time.of_datetime(self)
    end
end
