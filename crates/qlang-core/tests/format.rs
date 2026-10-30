//! `Format`: numbers and text laid out for people.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {} {}", d.code, d.message),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => e.render(),
    }
}

#[test]
fn numbers_with_separators_and_decimals() {
    let src = r#"
print(Format.number(1234567.891, 2))
print(Format.number(-0.004, 2))
print(Format.number(0.5, 0))
print(Format.number(1234.5, 3))
print(Format.number(-1234.5, 1))
print(Format.number(0.0, 2))
print(Format.number(5.0, 20))
print(Format.number_with(1234567.5, 1, ",", " "))
print(Format.number_with(1234.5, 2, ".", ""))
print(Format.number(1.0e30, 2))
print(Format.int(1234567))
print(Format.int(-1234))
print(Format.int(999))
print(Format.int(1000))
print(Format.int_with(1234567, "."))
"#;
    assert_eq!(
        run(src),
        "1,234,567.89\n0.00\n1\n1,234.500\n-1,234.5\n0.00\n5.000000000000000\n1 234 567,5\n1234.50\n1e30\n1,234,567\n-1,234\n999\n1,000\n1.234.567\n"
    );
}

#[test]
fn sizes_in_bytes() {
    let src = r#"
print(Format.bytes(0))
print(Format.bytes(512))
print(Format.bytes(1023))
print(Format.bytes(1024))
print(Format.bytes(1536))
print(Format.bytes(5 * 1024 * 1024 + 300000))
print(Format.bytes(-2048))
print(Format.bytes(3 * 1024 * 1024 * 1024))
"#;
    assert_eq!(
        run(src),
        "0 B\n512 B\n1023 B\n1.0 KB\n1.5 KB\n5.3 MB\n-2.0 KB\n3.0 GB\n"
    );
}

#[test]
fn sprintf_conversions_flags_width_and_precision() {
    let src = r#"
print(Format.sprintf("%-8s|%05.1f|", [Any.of("ab"), Any.of(3.14159)]))
print(Format.sprintf("%5d|%-5d|%05d|%+d|%x|%X|%o|%b", [Any.of(42), Any.of(42), Any.of(42), Any.of(42), Any.of(255), Any.of(255), Any.of(8), Any.of(5)]))
print(Format.sprintf("%s %s %.2s %% %f", [Any.of(1), Any.of(true), Any.of("abcdef"), Any.of(2.5)]))
print(Format.sprintf("%05d|%+.2f|%8.3f|%.0f", [Any.of(-42), Any.of(2), Any.of(3.14159), Any.of(2.5)]))
print(Format.sprintf("%10s|", [Any.of("right")]))
print(Format.sprintf("no conversion", []))
print(Format.sprintf("100%%", []))
"#;
    assert_eq!(
        run(src),
        "ab      |003.1|\n   42|42   |00042|+42|ff|FF|10|101\n1 true ab % 2.500000\n-0042|+2.00|   3.142|3\n     right|\nno conversion\n100%\n"
    );
}

#[test]
fn sprintf_mistakes_are_panics_that_say_what_is_wrong() {
    for (src, want) in [
        (
            r#"print(Format.sprintf("%d", [Any.of("x")]))"#,
            "`%d` needs an int",
        ),
        (
            r#"print(Format.sprintf("%f", [Any.of("x")]))"#,
            "`%f` needs a number",
        ),
        (
            r#"print(Format.sprintf("%d %d", [Any.of(1)]))"#,
            "has no value",
        ),
        (
            r#"print(Format.sprintf("%d", [Any.of(1), Any.of(2)]))"#,
            "1 value(s) were not used",
        ),
        (
            r#"print(Format.sprintf("%q", [Any.of(1)]))"#,
            "not a known conversion",
        ),
        (r#"print(Format.sprintf("50%", []))"#, "ends after a `%`"),
    ] {
        let out = run(src);
        assert!(
            out.starts_with("RUNTIME R050") && out.contains(want),
            "{src}: {out}"
        );
    }
}
