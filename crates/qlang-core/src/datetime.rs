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
    Parts { year, month, day, hour: rest / 3_600_000, minute: rest / 60_000 % 60, second: rest / 1000 % 60, ms: rest % 1000 }
}

/// The instant for these parts, or `None` if the date does not exist (30 February, hour 25,
/// year 0...).
pub fn from_parts(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, ms: i64) -> Option<i64> {
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
    Some(days_from_civil(year, month, day) * DAY_MS + hour * 3_600_000 + minute * 60_000 + second * 1000 + ms)
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
    let frac = if p.ms != 0 { format!(".{:03}", p.ms) } else { String::new() };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{frac}Z", p.year, p.month, p.day, p.hour, p.minute, p.second)
}

/// Formats with these tokens: `YYYY` `MM` `DD` `HH` `mm` `ss` `SSS`. Any other character is
/// copied as it is.
pub fn format(ms: i64, pattern: &str) -> String {
    let p = parts(ms);
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let run = chars[i..].iter().take_while(|c| **c == chars[i]).count();
        let (text, used) = match (chars[i], run) {
            ('Y', n) if n >= 4 => (format!("{:04}", p.year), 4),
            ('M', n) if n >= 2 => (format!("{:02}", p.month), 2),
            ('D', n) if n >= 2 => (format!("{:02}", p.day), 2),
            ('H', n) if n >= 2 => (format!("{:02}", p.hour), 2),
            ('m', n) if n >= 2 => (format!("{:02}", p.minute), 2),
            ('s', n) if n >= 2 => (format!("{:02}", p.second), 2),
            ('S', n) if n >= 3 => (format!("{:03}", p.ms), 3),
            (c, _) => (c.to_string(), 1),
        };
        out.push_str(&text);
        i += used;
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(from_parts(1970, 1, 1, 0, 0, 0, 0), Some(0));
        assert_eq!(from_parts(2023, 11, 14, 22, 13, 20, 0), Some(1_700_000_000_000));
        assert_eq!(from_parts(1, 1, 1, 0, 0, 0, 0), Some(MIN_MS));
        assert_eq!(from_parts(9999, 12, 31, 23, 59, 59, 999), Some(MAX_MS));
        assert_eq!(from_parts(1969, 12, 31, 23, 59, 59, 999), Some(-1));
    }

    #[test]
    fn parts_round_trip_across_the_whole_range() {
        let mut ms = MIN_MS;
        while ms <= MAX_MS {
            let p = parts(ms);
            assert_eq!(from_parts(p.year, p.month, p.day, p.hour, p.minute, p.second, p.ms), Some(ms), "{ms}");
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
        assert_eq!(to_iso(add_months(jan31, 1).unwrap()), "2026-02-28T10:00:00Z");
        assert_eq!(to_iso(add_months(jan31, 13).unwrap()), "2027-02-28T10:00:00Z");
        assert_eq!(to_iso(add_months(jan31, -2).unwrap()), "2025-11-30T10:00:00Z");
        assert_eq!(add_months(from_parts(9999, 12, 1, 0, 0, 0, 0).unwrap(), 1), None);
    }

    #[test]
    fn iso_text_and_parsing() {
        let t = from_parts(2026, 10, 11, 14, 30, 5, 0).unwrap();
        assert_eq!(to_iso(t), "2026-10-11T14:30:05Z");
        assert_eq!(parse("2026-10-11T14:30:05Z"), Some(t));
        assert_eq!(parse("2026-10-11 14:30:05"), Some(t));
        assert_eq!(parse("2026-10-11T14:30"), Some(t - 5000));
        assert_eq!(parse("2026-10-11"), Some(start_of_day(t)));
        assert_eq!(to_iso(parse("2026-10-11T14:30:05.042Z").unwrap()), "2026-10-11T14:30:05.042Z");
        for bad in ["", "2026-10", "2026-13-01", "2026-02-30", "2026-10-11T25:00", "2026-10-11T14:30:05+02:00", "2026/10/11", "26-10-11", "2026-10-11x"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn format_tokens() {
        let t = from_parts(2026, 3, 9, 4, 5, 6, 7).unwrap();
        assert_eq!(format(t, "YYYY-MM-DD HH:mm:ss.SSS"), "2026-03-09 04:05:06.007");
        assert_eq!(format(t, "DD/MM/YYYY"), "09/03/2026");
        assert_eq!(format(t, "at HH:mm"), "at 04:05");
        assert_eq!(format(t, "M D Y"), "M D Y");
    }
}
