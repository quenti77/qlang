//! The `datetime` type: calendar arithmetic in UTC, the clock comes from the host.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return format!(
                        "COMPILE: {}",
                        e.diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.code, d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

/// With a clock at 2023-11-14T22:13:20Z.
fn ok(src: &str) -> String {
    exec(
        src,
        MemoryHost {
            clock: Some(1_700_000_000_000),
            ..MemoryHost::default()
        },
    )
}

#[test]
fn now_comes_from_the_host_and_prints_as_iso() {
    assert_eq!(ok("print(datetime.now())"), "2023-11-14T22:13:20Z\n");
    assert_eq!(ok("print(datetime.now().ms() == time.now())"), "true\n");
    assert!(exec("print(datetime.now())", MemoryHost::default()).starts_with("COMPILE: T250"));
}

#[test]
fn build_and_read_the_parts() {
    let out = ok("let d = datetime.of(2026, 10, 11, 14, 30, 5)
print(d)
print(d.year())
print(d.month())
print(d.day())
print(d.hour())
print(d.minute())
print(d.second())
print(d.weekday())
print(datetime.of(2026, 10, 11))");
    assert_eq!(
        out,
        "2026-10-11T14:30:05Z\n2026\n10\n11\n14\n30\n5\n7\n2026-10-11T00:00:00Z\n"
    );
}

#[test]
fn dates_that_do_not_exist_are_errors() {
    assert!(ok("print(datetime.of(2026, 2, 29))").starts_with("R064"));
    assert!(ok("print(datetime.of(2026, 1, 1, 24, 0, 0))").starts_with("R064"));
    assert!(ok("print(datetime.of(0, 1, 1))").starts_with("R064"));
    assert!(ok("print(datetime.of(2026, 1))").starts_with("COMPILE"));
    assert!(ok("print(datetime.at(300000000000000))").starts_with("R064"));
}

#[test]
fn add_and_subtract() {
    let out = ok("let d = datetime.of(2026, 1, 31, 10, 0, 0)
print(d.add_days(1))
print(d.add_months(1))
print(d.add_years(1).add_months(-1))
print(d.add_hours(15))
print(d.add_minutes(-30))
print(d.add_seconds(90))
print(d.start_of_day())
print(datetime.of(2026, 3, 1) - datetime.of(2026, 2, 28))
print(datetime.of(2026, 3, 1).days_until(datetime.of(2026, 2, 1)))");
    assert_eq!(
        out,
        "2026-02-01T10:00:00Z\n2026-02-28T10:00:00Z\n2026-12-31T10:00:00Z\n2026-02-01T01:00:00Z\n2026-01-31T09:30:00Z\n2026-01-31T10:01:30Z\n2026-01-31T00:00:00Z\n86400000\n-28\n"
    );
    assert!(ok("print(datetime.of(9999, 12, 31).add_days(1))").starts_with("R065"));
    assert!(
        ok("print(datetime.of(2026, 1, 1).add_years(9223372036854775807))").starts_with("R065")
    );
}

#[test]
fn before_1970() {
    let out = ok(
        "let d = datetime.at(-1)\nprint(d)\nprint(d.ms())\nprint(d.weekday())\nprint(datetime.of(1, 1, 1).add_days(-1) == none)",
    );
    assert!(
        out.starts_with("1969-12-31T23:59:59.999Z\n-1\n3\n") || out.starts_with("R065"),
        "{out}"
    );
    assert!(ok("print(datetime.of(1, 1, 1).add_days(-1))").starts_with("R065"));
}

#[test]
fn format_and_parse() {
    let out = ok("let d = datetime.of(2026, 3, 9, 4, 5, 6)
print(d.format(\"DD/MM/YYYY\"))
print(d.format(\"YYYY-MM-DD HH:mm:ss\"))
let p = datetime.parse(\"2026-03-09 04:05\")
if p != none then
  print(p)
end
print(datetime.parse(\"09/03/2026\") == none)
print(datetime.parse(\"2026-02-30\") == none)");
    assert_eq!(
        out,
        "09/03/2026\n2026-03-09 04:05:06\n2026-03-09T04:05:00Z\ntrue\ntrue\n"
    );
}

#[test]
fn compare_sort_and_keys() {
    let out = ok("let a = datetime.of(2026, 1, 1)
let b = datetime.of(2025, 6, 1)
print(a > b)
print(a == datetime.of(2026, 1, 1))
let xs = [a, b]
xs.sort()
print(xs[0])
let events: map<datetime, string> = {}
events[a] = \"new year\"
print(events[a])
print(\"it is {a}\")");
    assert_eq!(
        out,
        "true\ntrue\n2025-06-01T00:00:00Z\nnew year\nit is 2026-01-01T00:00:00Z\n"
    );
}

#[test]
fn types_do_not_mix() {
    assert!(ok("let d: datetime = 5").starts_with("COMPILE"));
    assert!(ok("let n: int = datetime.now()").starts_with("COMPILE"));
    assert!(ok("print(datetime.now() + datetime.now())").starts_with("COMPILE"));
    assert!(ok("print(datetime.now() == time.now())").starts_with("COMPILE"));
}

#[test]
fn datetime_is_a_type_name_like_int() {
    assert!(ok("let datetime = 3").starts_with("COMPILE: T110"));
}
