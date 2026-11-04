//! `Duration`, and what `datetime` gains: comparisons, differences, `start_of_*`, `with`, `+` and `-`.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// The output of the program, or `COMPILE codes` / `RUNTIME code`.
fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            format!("COMPILE {}", codes.join(","))
        }
    }
}

#[test]
fn a_duration_is_added_to_a_datetime_months_first() {
    let src = "let t = datetime.of(2026, 1, 31, 10, 0, 0)
print(t + Duration.of(months: 1))
print(t + Duration.of(months: 1, days: 2, hours: 3))
print(t - Duration.of(days: 31))
print(t.plus(Duration.of(ms: 1500)))
";
    assert_eq!(
        run(src),
        "2026-02-28T10:00:00Z\n2026-03-02T13:00:00Z\n2025-12-31T10:00:00Z\n2026-01-31T10:00:01.500Z\n"
    );
}

#[test]
fn totals_need_fixed_lengths() {
    let src = "let d = Duration.of(days: 1, hours: 2, minutes: 30)
print(d.total_minutes().unwrap())
print(d.total_hours().unwrap())
print(Duration.of(months: 1).total_ms().is_err())
print(Duration.of(minutes: 90).normalize().to_iso())
print(Duration.of(years: 1, months: 14, ms: 7).normalize().to_iso())
print(Duration.of().to_iso())
print(Duration.of(days: 2, hours: -3).normalize().to_iso())
print(Duration.of(hours: 1).times(3).negate().to_iso())
";
    assert_eq!(
        run(src),
        "1590\n26\ntrue\nPT1H30M\nP2Y2MT0.007S\nPT0S\nP1DT21H\n-PT3H\n"
    );
}

#[test]
fn comparisons_and_calendar_helpers() {
    let src = "let a = datetime.of(2026, 3, 9)
let b = datetime.of(2026, 3, 10)
print(a.is_before(b))
print(a.is_after(b))
print(a.between(a, b))
print(a.same_day(a.add_hours(5)))
print(a.is_weekend())
print(datetime.of(2026, 3, 14).is_weekend())
print(datetime.of(2024, 2, 1).days_in_month())
print(datetime.of(2026, 11, 3).quarter())
print(a.is_leap_year())
";
    assert_eq!(
        run(src),
        "true\nfalse\ntrue\ntrue\nfalse\ntrue\n29\n4\nfalse\n"
    );
}

#[test]
fn differences_are_whole_units() {
    let src = "let a = datetime.of(2026, 1, 31)
print(a.diff_in_months(datetime.of(2026, 2, 28)))
print(a.diff_in_months(datetime.of(2026, 2, 27)))
print(datetime.of(2026, 3, 31).diff_in_months(a))
print(a.diff_in_years(datetime.of(2028, 1, 30)))
print(a.diff_in_days(datetime.of(2026, 3, 2)))
print(a.diff_in_hours(datetime.of(2026, 2, 1, 5, 0, 0)))
print(a.until(datetime.of(2027, 3, 4, 5, 6, 7)).to_iso())
print(datetime.of(2027, 3, 4).until(a).to_iso())
print(a.until(a).to_iso())
";
    assert_eq!(
        run(src),
        "1\n0\n-2\n1\n30\n29\nP1Y1M4DT5H6M7S\n-P1Y1M4D\nPT0S\n"
    );
}

#[test]
fn start_and_end_of_a_period() {
    let src = "let t = datetime.of(2026, 11, 4, 15, 30, 20)
print(t.start_of_hour())
print(t.end_of_hour())
print(t.end_of_day())
print(t.start_of_week())
print(t.end_of_week())
print(t.start_of_month())
print(t.end_of_month())
print(t.start_of_quarter())
print(t.end_of_quarter())
print(t.start_of_year())
print(t.end_of_year())
";
    assert_eq!(
        run(src),
        "2026-11-04T15:00:00Z\n2026-11-04T15:59:59.999Z\n2026-11-04T23:59:59.999Z\n\
         2026-11-02T00:00:00Z\n2026-11-08T23:59:59.999Z\n2026-11-01T00:00:00Z\n\
         2026-11-30T23:59:59.999Z\n2026-10-01T00:00:00Z\n2026-12-31T23:59:59.999Z\n\
         2026-01-01T00:00:00Z\n2026-12-31T23:59:59.999Z\n"
    );
}

#[test]
fn with_replaces_parts_and_weekdays_are_found() {
    let src = "let t = datetime.of(2026, 1, 31, 10, 20, 30)
print(t.with(month: 2))
print(t.with(month: 3, day: 1, hour: 0))
print(t.next_weekday(1))
print(t.previous_weekday(1))
print(t.nth_weekday_of_month(2, 1))
print(t.nth_weekday_of_month(5, 1))
print(t.nth_weekday_of_month(5, 6))
print(t.last_day_of_month())
";
    assert_eq!(
        run(src),
        "2026-02-28T10:20:30Z\n2026-03-01T00:20:30Z\n2026-02-02T00:00:00Z\n2026-01-26T00:00:00Z\n\
         2026-01-12T00:00:00Z\nnone\n2026-01-31T00:00:00Z\n2026-01-31T00:00:00Z\n"
    );
}

#[test]
fn text_forms_and_unix_time() {
    let src = "let t = datetime.of(2026, 11, 3, 14, 30, 0)
print(t.to_http())
print(t.to_rfc2822())
print(t.to_iso())
print(t.unix())
print(datetime.at(-1).unix())
print(t.format_in(\"dddd D MMMM YYYY\", \"fr\"))
print(t.week())
";
    assert_eq!(
        run(src),
        "Tue, 03 Nov 2026 14:30:00 GMT\nTue, 03 Nov 2026 14:30:00 +0000\n2026-11-03T14:30:00Z\n\
         1793716200\n-1\nmardi 3 novembre 2026\n45\n"
    );
}

#[test]
fn humanize_says_how_long_ago() {
    let src = "let now = datetime.of(2026, 11, 3, 12, 0, 0)
print(now.add_seconds(-10).humanize(now))
print(now.add_minutes(-5).humanize(now))
print(now.add_hours(3).humanize(now))
print(now.add_days(-3).humanize(now))
print(now.add_months(-2).humanize(now))
print(now.add_years(1).humanize(now))
print(now.add_days(-3).humanize(now, \"fr\"))
print(now.add_months(2).humanize(now, \"fr\"))
";
    assert_eq!(
        run(src),
        "a few seconds ago\n5 minutes ago\nin 3 hours\n3 days ago\n2 months ago\nin 1 year\n\
         il y a 3 jours\ndans 2 mois\n"
    );
}

#[test]
fn an_unknown_language_is_an_error() {
    assert_eq!(
        run("print(datetime.of(2026, 1, 1).format_in(\"MMMM\", \"xx\"))"),
        "RUNTIME R066"
    );
}
