//! `BigInt` (an integer without limit) and `BigDecimal` (an exact decimal, for money).

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
fn big_numbers_are_exact_past_the_int_range() {
    let src = "let f = BigInt.one()
for i in 1..101 do
    f = f * i
end
print(f)
print(BigInt.pow(2, 200))
let a = BigInt.parse(\"123456789012345678901234567890\").unwrap()
print(a * a)
print(a + 1)
print(a - a)
print(-a)
print(BigInt.pow(3, 300))
";
    assert_eq!(
        run(src),
        "93326215443944152681699238856266700490715968264381621468592963895217599993229915608941463976156518286253697920827223758251185210916864000000000000000000000000\n\
         1606938044258990275541962092341162602522202993782792835301376\n\
         15241578753238836750495351562536198787501905199875019052100\n\
         123456789012345678901234567891\n0\n-123456789012345678901234567890\n\
         136891479058588375991326027382088315966463695625337436471480190078368997177499076593800206155688941388250484440597994042813512732765695774566001\n"
    );
}

#[test]
fn division_cuts_toward_zero_like_int() {
    let src = "print(BigInt.pow(2, 200) div BigInt.pow(2, 100))
print((BigInt.pow(2, 200) + 5) % BigInt.pow(2, 100))
print(BigInt.of(-7) div 2)
print(BigInt.of(-7) % 2)
print(BigInt.of(7) div -2)
print(BigInt.pow(10, 40) div 7)
print(BigInt.pow(10, 40) % 7)
let pair = BigInt.of(-7).divmod(BigInt.of(2))
print(pair.0)
print(pair.1)
";
    assert_eq!(
        run(src),
        "1267650600228229401496703205376\n5\n-3\n-1\n-3\n1428571428571428571428571428571428571428\n4\n-3\n-1\n"
    );
}

#[test]
fn mixing_with_int_and_comparing() {
    let src = "print(BigInt.of(3) + 2)
print(2 + BigInt.of(3))
print(10 - BigInt.of(3))
print(4 * BigInt.of(5))
print(BigInt.of(2) ** 70)
print(BigInt.of(3) + 2 == BigInt.of(5))
print(BigInt.of(3) < BigInt.of(5))
print(BigInt.of(-3) < BigInt.of(-5))
print(BigInt.pow(2, 100) > BigInt.pow(2, 99))
print(\"{BigInt.of(42)} and \" + (BigInt.of(7) as string))
";
    assert_eq!(
        run(src),
        "5\n5\n7\n20\n1180591620717411303424\ntrue\ntrue\nfalse\ntrue\n42 and 7\n"
    );
}

#[test]
fn conversions_to_int_are_checked() {
    let src = "print(BigInt.of(9223372036854775807).to_int().unwrap())
print(BigInt.of(-9223372036854775807 - 1).to_int().unwrap())
print((BigInt.of(9223372036854775807) + 1).to_int().is_err())
print((BigInt.of(-9223372036854775807 - 1) - 1).to_int().is_err())
print(BigInt.parse(\"12x\").is_err())
print(BigInt.parse(\"\").is_err())
print(BigInt.parse(\"-0\").unwrap())
print(BigInt.parse(\"+000123\").unwrap())
print(BigInt.of(0).is_zero())
";
    assert_eq!(
        run(src),
        "9223372036854775807\n-9223372036854775808\ntrue\ntrue\ntrue\ntrue\n0\n123\ntrue\n"
    );
}

#[test]
fn dividing_a_big_int_by_zero_stops_the_program() {
    let src = "print(BigInt.of(1) div BigInt.of(0))\n";
    assert!(run(src).starts_with("RUNTIME"));
}

#[test]
fn slash_is_not_defined_on_a_big_int() {
    assert!(run("print(BigInt.of(1) / BigInt.of(2))\n").starts_with("COMPILE"));
}

#[test]
fn a_decimal_adds_exactly() {
    let src = "let a = BigDecimal.parse(\"0.1\").unwrap()
let b = BigDecimal.parse(\"0.2\").unwrap()
print(a + b)
print(a + b == BigDecimal.parse(\"0.3\").unwrap())
print(BigDecimal.parse(\"12.50\").unwrap())
print(BigDecimal.parse(\"12.50\").unwrap() == BigDecimal.parse(\"12.5\").unwrap())
print(BigDecimal.parse(\"12.50\").unwrap().strip_zeros())
let price = BigDecimal.parse(\"19.99\").unwrap()
print(price * 3)
print(price * BigDecimal.parse(\"0.2\").unwrap())
print(BigDecimal.parse(\"-0.05\").unwrap())
print(1 + BigDecimal.parse(\"0.5\").unwrap())
print(BigDecimal.parse(\"5\").unwrap() - BigDecimal.parse(\"0.25\").unwrap())
print(BigDecimal.parse(\"1.5\").unwrap() < BigDecimal.parse(\"1.25\").unwrap())
";
    assert_eq!(
        run(src),
        "0.3\ntrue\n12.50\ntrue\n12.5\n59.97\n3.998\n-0.05\n1.5\n4.75\nfalse\n"
    );
}

#[test]
fn division_and_rounding_need_a_scale_and_a_mode() {
    let src = "let ten = BigDecimal.parse(\"10\").unwrap()
let two = BigDecimal.parse(\"2\").unwrap()
print(ten.divide(BigDecimal.of(3), 4, \"half_even\"))
print(two.divide(BigDecimal.of(3), 2, \"up\"))
print(two.negate().divide(BigDecimal.of(3), 2, \"floor\"))
print(two.negate().divide(BigDecimal.of(3), 2, \"ceiling\"))
print(BigDecimal.parse(\"2.345\").unwrap().round(2, \"half_even\"))
print(BigDecimal.parse(\"2.355\").unwrap().round(2, \"half_even\"))
print(BigDecimal.parse(\"2.355\").unwrap().round(2, \"half_up\"))
print(BigDecimal.parse(\"2.355\").unwrap().round(2, \"half_down\"))
print(BigDecimal.parse(\"2.399\").unwrap().round(2, \"down\"))
print(BigDecimal.parse(\"2.4\").unwrap().round(3, \"down\"))
print(BigDecimal.parse(\"-2.5\").unwrap().to_bigint(\"down\"))
print(BigDecimal.parse(\"1.5\").unwrap().to_float())
";
    assert_eq!(
        run(src),
        "3.3333\n0.67\n-0.67\n-0.66\n2.34\n2.36\n2.36\n2.35\n2.39\n2.400\n-2\n1.5\n"
    );
}

#[test]
fn bad_decimals_and_roundings_are_refused() {
    assert_eq!(
        run(
            "print(BigDecimal.parse(\"1.2.3\").is_err())\nprint(BigDecimal.parse(\"1.\").is_err())\nprint(BigDecimal.parse(\"abc\").is_err())\n"
        ),
        "true\ntrue\ntrue\n"
    );
    assert!(run("print(BigDecimal.of(1).round(0, \"sideways\"))\nprint(BigDecimal.of(1).divide(BigDecimal.of(3), 2, \"sideways\"))\n").starts_with("RUNTIME"));
    assert!(run("print(BigDecimal.of(1) / BigDecimal.of(2))\n").starts_with("COMPILE"));
}

#[test]
fn big_numbers_make_json_strings() {
    let src = "print(BigInt.pow(2, 70).to_json())
print(BigDecimal.parse(\"12.50\").unwrap().to_json())
";
    assert_eq!(run(src), "\"1180591620717411303424\"\n\"12.50\"\n");
}
