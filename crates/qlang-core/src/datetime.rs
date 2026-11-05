//! `datetime`: an instant, in milliseconds since 1970-01-01 00:00:00 UTC (always UTC), from
//! the year 1 to the year 9999. Pure calendar arithmetic: the clock itself comes from the host.

pub const DAY_MS: i64 = 86_400_000;
/// 0001-01-01T00:00:00.000Z
pub const MIN_MS: i64 = -62_135_596_800_000;
/// 9999-12-31T23:59:59.999Z
pub const MAX_MS: i64 = 253_402_300_799_999;

pub fn in_range(ms: i64) -> bool {
    (MIN_MS..=MAX_MS).contains(&ms)
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian calendar).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Parts {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub ms: i64,
}

pub fn parts(ms: i64) -> Parts {
    let days = ms.div_euclid(DAY_MS);
    let rest = ms.rem_euclid(DAY_MS);
    let (year, month, day) = civil_from_days(days);
    Parts {
        year,
        month,
        day,
        hour: rest / 3_600_000,
        minute: rest / 60_000 % 60,
        second: rest / 1000 % 60,
        ms: rest % 1000,
    }
}

/// The instant for these parts, or `None` if the date does not exist (30 February, hour 25,
/// year 0...).
pub fn from_parts(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    ms: i64,
) -> Option<i64> {
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
        || !(0..1000).contains(&ms)
    {
        return None;
    }
    Some(
        days_from_civil(year, month, day) * DAY_MS
            + hour * 3_600_000
            + minute * 60_000
            + second * 1000
            + ms,
    )
}

/// Monday is 1, Sunday is 7.
pub fn weekday(ms: i64) -> i64 {
    // 1970-01-01 was a Thursday (4)
    (ms.div_euclid(DAY_MS) + 3).rem_euclid(7) + 1
}

pub fn start_of_day(ms: i64) -> i64 {
    ms.div_euclid(DAY_MS) * DAY_MS
}

/// Adds months; a day that does not exist in the new month falls on its last day
/// (31 January + 1 month = 28 or 29 February).
pub fn add_months(ms: i64, n: i64) -> Option<i64> {
    let p = parts(ms);
    let total = (p.year * 12 + (p.month - 1)).checked_add(n)?;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if !(1..=9999).contains(&year) {
        return None;
    }
    let day = p.day.min(days_in_month(year, month));
    from_parts(year, month, day, p.hour, p.minute, p.second, p.ms)
}

/// `2026-10-11T14:30:00Z`, with `.123` before the `Z` when there are milliseconds.
pub fn to_iso(ms: i64) -> String {
    let p = parts(ms);
    let frac = if p.ms != 0 {
        format!(".{:03}", p.ms)
    } else {
        String::new()
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{frac}Z",
        p.year, p.month, p.day, p.hour, p.minute, p.second
    )
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
const DAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const DAYS_FR: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];

const MONTHS_ES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const DAYS_ES: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];
const MONTHS_DE: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];
const DAYS_DE: [&str; 7] = [
    "Montag",
    "Dienstag",
    "Mittwoch",
    "Donnerstag",
    "Freitag",
    "Samstag",
    "Sonntag",
];
const MONTHS_IT: [&str; 12] = [
    "gennaio",
    "febbraio",
    "marzo",
    "aprile",
    "maggio",
    "giugno",
    "luglio",
    "agosto",
    "settembre",
    "ottobre",
    "novembre",
    "dicembre",
];
const DAYS_IT: [&str; 7] = [
    "lunedì",
    "martedì",
    "mercoledì",
    "giovedì",
    "venerdì",
    "sabato",
    "domenica",
];
const MONTHS_PT: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];
const DAYS_PT: [&str; 7] = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
];
const MONTHS_NL: [&str; 12] = [
    "januari",
    "februari",
    "maart",
    "april",
    "mei",
    "juni",
    "juli",
    "augustus",
    "september",
    "oktober",
    "november",
    "december",
];
const DAYS_NL: [&str; 7] = [
    "maandag",
    "dinsdag",
    "woensdag",
    "donderdag",
    "vrijdag",
    "zaterdag",
    "zondag",
];

/// The names of the months and of the weekdays (Monday first) in a language: `en`, `fr`, `es`, `de`,
/// `it`, `pt` and `nl`.
pub fn names(locale: &str) -> Option<([&'static str; 12], [&'static str; 7])> {
    match locale {
        "en" => Some((MONTHS_EN, DAYS_EN)),
        "fr" => Some((MONTHS_FR, DAYS_FR)),
        "es" => Some((MONTHS_ES, DAYS_ES)),
        "de" => Some((MONTHS_DE, DAYS_DE)),
        "it" => Some((MONTHS_IT, DAYS_IT)),
        "pt" => Some((MONTHS_PT, DAYS_PT)),
        "nl" => Some((MONTHS_NL, DAYS_NL)),
        _ => None,
    }
}

/// The first three characters of a name (`Mon`, `janv`-less: `jan`).
fn short(name: &str) -> String {
    name.chars().take(3).collect()
}

/// 1 January is day 1.
pub fn day_of_year(ms: i64) -> i64 {
    let p = parts(ms);
    (days_from_civil(p.year, p.month, p.day) - days_from_civil(p.year, 1, 1)) + 1
}

/// The ISO 8601 week (1 to 53) and the year it belongs to: the week with the year's first Thursday
/// is week 1.
pub fn iso_week(ms: i64) -> (i64, i64) {
    let p = parts(ms);
    let wd = weekday(ms);
    let thursday = days_from_civil(p.year, p.month, p.day) + (4 - wd);
    let (year, _, _) = civil_from_days(thursday);
    let week = (thursday - days_from_civil(year, 1, 1)) / 7 + 1;
    (week, year)
}

/// Formats with these tokens: `YYYY` `YY` year, `MMMM` `MMM` the month's name, `MM` `M` its number,
/// `DD` `D` day, `dddd` `ddd` weekday name, `HH` `H` hour, `hh` `h` hour on 12 hours, `A` AM or PM,
/// `mm` minute, `ss` second, `SSS` milliseconds, `WW` ISO week. Text between `[` and `]` is kept
/// as it is. Any other character is copied as it is.
pub fn format(ms: i64, pattern: &str) -> String {
    format_locale(ms, pattern, "en").unwrap_or_default()
}

/// Like [`format`], with the names of months and weekdays in `locale` (`None` for an unknown one).
pub fn format_locale(ms: i64, pattern: &str, locale: &str) -> Option<String> {
    let (months, days) = names(locale)?;
    let p = parts(ms);
    let hour12 = if p.hour % 12 == 0 { 12 } else { p.hour % 12 };
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '['
            && let Some(close) = chars[i + 1..].iter().position(|c| *c == ']')
        {
            out.extend(&chars[i + 1..i + 1 + close]);
            i += close + 2;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| **c == chars[i]).count();
        let month = months[(p.month - 1) as usize];
        let day = days[(weekday(ms) - 1) as usize];
        let (text, used) = match (chars[i], run) {
            ('Y', n) if n >= 4 => (format!("{:04}", p.year), 4),
            ('Y', 2 | 3) => (format!("{:02}", p.year % 100), 2),
            ('M', n) if n >= 4 => (month.to_string(), 4),
            ('M', 3) => (short(month), 3),
            ('M', 2) => (format!("{:02}", p.month), 2),
            ('M', 1) => (p.month.to_string(), 1),
            ('D', n) if n >= 2 => (format!("{:02}", p.day), 2),
            ('D', 1) => (p.day.to_string(), 1),
            ('d', n) if n >= 4 => (day.to_string(), 4),
            ('d', 3) => (short(day), 3),
            ('H', n) if n >= 2 => (format!("{:02}", p.hour), 2),
            ('H', 1) => (p.hour.to_string(), 1),
            ('h', n) if n >= 2 => (format!("{hour12:02}"), 2),
            ('h', 1) => (hour12.to_string(), 1),
            ('A', _) => (if p.hour < 12 { "AM" } else { "PM" }.to_string(), 1),
            ('m', n) if n >= 2 => (format!("{:02}", p.minute), 2),
            ('s', n) if n >= 2 => (format!("{:02}", p.second), 2),
            ('S', n) if n >= 3 => (format!("{:03}", p.ms), 3),
            ('W', n) if n >= 2 => (format!("{:02}", iso_week(ms).0), 2),
            (c, _) => (c.to_string(), 1),
        };
        out.push_str(&text);
        i += used;
    }
    Some(out)
}

/// `YYYY-MM-DD`, optionally followed by `T` or a space and `HH:MM`, `HH:MM:SS` or
/// `HH:MM:SS.mmm`, optionally followed by `Z`. A date alone is midnight.
pub fn parse(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let s = b.get(from..from + len)?;
        if !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(s).ok()?.parse().ok()
    };
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let mut end = 10;
    let (mut hour, mut minute, mut second, mut ms) = (0, 0, 0, 0);
    if matches!(b.get(10), Some(b'T' | b' ')) {
        if b.get(13) != Some(&b':') {
            return None;
        }
        hour = num(11, 2)?;
        minute = num(14, 2)?;
        end = 16;
        if b.get(16) == Some(&b':') {
            second = num(17, 2)?;
            end = 19;
            if b.get(19) == Some(&b'.') {
                ms = num(20, 3)?;
                end = 23;
            }
        }
    }
    if b.get(end) == Some(&b'Z') {
        end += 1;
    }
    if end != b.len() {
        return None;
    }
    from_parts(year, month, day, hour, minute, second, ms)
}

/// Reads `text` written with `pattern`: the tokens of [`format`] `YYYY` `YY` `MM` `M` `DD` `D` `HH` `H`
/// `mm` `ss` `SSS`, and `ZZ` for an offset (`+02:00`, `-0530` or `Z`, which is taken away to give UTC). Text
/// between `[` and `]` and any other character must be there as they are. `None` if `text` does not fit.
pub fn parse_with(text: &str, pattern: &str) -> Option<i64> {
    let t: Vec<char> = text.chars().collect();
    let pat: Vec<char> = pattern.chars().collect();
    let (mut at, mut i) = (0, 0);
    let (mut year, mut month, mut day) = (1970, 1, 1);
    let (mut hour, mut minute, mut second, mut ms, mut offset) = (0, 0, 0, 0, 0);
    let digits = |at: &mut usize, min: usize, max: usize| -> Option<i64> {
        let n = t[*at..]
            .iter()
            .take(max)
            .take_while(|c| c.is_ascii_digit())
            .count();
        if n < min {
            return None;
        }
        let v = t[*at..*at + n].iter().collect::<String>().parse().ok()?;
        *at += n;
        Some(v)
    };
    while i < pat.len() {
        if pat[i] == '['
            && let Some(close) = pat[i + 1..].iter().position(|c| *c == ']')
        {
            for c in &pat[i + 1..i + 1 + close] {
                if t.get(at) != Some(c) {
                    return None;
                }
                at += 1;
            }
            i += close + 2;
            continue;
        }
        let run = pat[i..].iter().take_while(|c| **c == pat[i]).count();
        let used = match (pat[i], run) {
            ('Y', n) if n >= 4 => {
                year = digits(&mut at, 4, 4)?;
                4
            }
            ('Y', 2 | 3) => {
                year = 2000 + digits(&mut at, 2, 2)?;
                2
            }
            ('M', n) if n >= 2 => {
                month = digits(&mut at, 2, 2)?;
                2
            }
            ('M', 1) => {
                month = digits(&mut at, 1, 2)?;
                1
            }
            ('D', n) if n >= 2 => {
                day = digits(&mut at, 2, 2)?;
                2
            }
            ('D', 1) => {
                day = digits(&mut at, 1, 2)?;
                1
            }
            ('H', n) if n >= 2 => {
                hour = digits(&mut at, 2, 2)?;
                2
            }
            ('H', 1) => {
                hour = digits(&mut at, 1, 2)?;
                1
            }
            ('m', n) if n >= 2 => {
                minute = digits(&mut at, 2, 2)?;
                2
            }
            ('s', n) if n >= 2 => {
                second = digits(&mut at, 2, 2)?;
                2
            }
            ('S', n) if n >= 3 => {
                ms = digits(&mut at, 3, 3)?;
                3
            }
            ('Z', n) if n >= 2 => {
                if t.get(at) == Some(&'Z') {
                    at += 1;
                } else {
                    let sign = match t.get(at)? {
                        '+' => 1,
                        '-' => -1,
                        _ => return None,
                    };
                    at += 1;
                    let h = digits(&mut at, 2, 2)?;
                    if t.get(at) == Some(&':') {
                        at += 1;
                    }
                    let m = digits(&mut at, 2, 2)?;
                    if h > 23 || m > 59 {
                        return None;
                    }
                    offset = sign * (h * 60 + m);
                }
                2
            }
            (c, _) => {
                if t.get(at) != Some(&c) {
                    return None;
                }
                at += 1;
                1
            }
        };
        i += used;
    }
    if at != t.len() {
        return None;
    }
    let utc = from_parts(year, month, day, hour, minute, second, ms)?;
    Some(utc - offset * 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(from_parts(1970, 1, 1, 0, 0, 0, 0), Some(0));
        assert_eq!(
            from_parts(2023, 11, 14, 22, 13, 20, 0),
            Some(1_700_000_000_000)
        );
        assert_eq!(from_parts(1, 1, 1, 0, 0, 0, 0), Some(MIN_MS));
        assert_eq!(from_parts(9999, 12, 31, 23, 59, 59, 999), Some(MAX_MS));
        assert_eq!(from_parts(1969, 12, 31, 23, 59, 59, 999), Some(-1));
    }

    #[test]
    fn parts_round_trip_across_the_whole_range() {
        let mut ms = MIN_MS;
        while ms <= MAX_MS {
            let p = parts(ms);
            assert_eq!(
                from_parts(p.year, p.month, p.day, p.hour, p.minute, p.second, p.ms),
                Some(ms),
                "{ms}"
            );
            ms += 86_399_999 * 13;
        }
    }

    #[test]
    fn invalid_dates_do_not_exist() {
        assert_eq!(from_parts(2026, 2, 29, 0, 0, 0, 0), None);
        assert!(from_parts(2028, 2, 29, 0, 0, 0, 0).is_some());
        assert_eq!(from_parts(1900, 2, 29, 0, 0, 0, 0), None);
        assert!(from_parts(2000, 2, 29, 0, 0, 0, 0).is_some());
        assert_eq!(from_parts(2026, 13, 1, 0, 0, 0, 0), None);
        assert_eq!(from_parts(2026, 1, 1, 24, 0, 0, 0), None);
        assert_eq!(from_parts(0, 1, 1, 0, 0, 0, 0), None);
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekday(0), 4); // Thursday
        assert_eq!(weekday(from_parts(2026, 10, 11, 0, 0, 0, 0).unwrap()), 7); // Sunday
        assert_eq!(weekday(-1), 3); // Wednesday
    }

    #[test]
    fn months_clamp_to_the_end_of_the_month() {
        let jan31 = from_parts(2026, 1, 31, 10, 0, 0, 0).unwrap();
        assert_eq!(
            to_iso(add_months(jan31, 1).unwrap()),
            "2026-02-28T10:00:00Z"
        );
        assert_eq!(
            to_iso(add_months(jan31, 13).unwrap()),
            "2027-02-28T10:00:00Z"
        );
        assert_eq!(
            to_iso(add_months(jan31, -2).unwrap()),
            "2025-11-30T10:00:00Z"
        );
        assert_eq!(
            add_months(from_parts(9999, 12, 1, 0, 0, 0, 0).unwrap(), 1),
            None
        );
    }

    #[test]
    fn iso_text_and_parsing() {
        let t = from_parts(2026, 10, 11, 14, 30, 5, 0).unwrap();
        assert_eq!(to_iso(t), "2026-10-11T14:30:05Z");
        assert_eq!(parse("2026-10-11T14:30:05Z"), Some(t));
        assert_eq!(parse("2026-10-11 14:30:05"), Some(t));
        assert_eq!(parse("2026-10-11T14:30"), Some(t - 5000));
        assert_eq!(parse("2026-10-11"), Some(start_of_day(t)));
        assert_eq!(
            to_iso(parse("2026-10-11T14:30:05.042Z").unwrap()),
            "2026-10-11T14:30:05.042Z"
        );
        for bad in [
            "",
            "2026-10",
            "2026-13-01",
            "2026-02-30",
            "2026-10-11T25:00",
            "2026-10-11T14:30:05+02:00",
            "2026/10/11",
            "26-10-11",
            "2026-10-11x",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn format_tokens() {
        let t = from_parts(2026, 3, 9, 4, 5, 6, 7).unwrap();
        assert_eq!(
            format(t, "YYYY-MM-DD HH:mm:ss.SSS"),
            "2026-03-09 04:05:06.007"
        );
        assert_eq!(format(t, "DD/MM/YYYY"), "09/03/2026");
        assert_eq!(format(t, "at HH:mm"), "at 04:05");
        assert_eq!(format(t, "D MMMM YYYY"), "9 March 2026");
        assert_eq!(format(t, "ddd D MMM YY, h:mm A"), "Mon 9 Mar 26, 4:05 AM");
        assert_eq!(format(t, "[on] dddd"), "on Monday");
        assert_eq!(format(t, "Y"), "Y");
        assert_eq!(
            format_locale(t, "dddd D MMMM", "fr").as_deref(),
            Some("lundi 9 mars")
        );
        assert_eq!(format_locale(t, "MMMM", "xx"), None);
    }

    #[test]
    fn iso_weeks_and_days_of_year() {
        let d = |y, m, d| from_parts(y, m, d, 0, 0, 0, 0).unwrap();
        assert_eq!(iso_week(d(2026, 1, 1)), (1, 2026));
        assert_eq!(iso_week(d(2024, 12, 30)), (1, 2025));
        assert_eq!(iso_week(d(2021, 1, 3)), (53, 2020));
        assert_eq!(day_of_year(d(2026, 12, 31)), 365);
        assert_eq!(day_of_year(d(2024, 12, 31)), 366);
    }
}
