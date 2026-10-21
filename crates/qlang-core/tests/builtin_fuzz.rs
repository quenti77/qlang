//! Built-in methods called with extreme arguments must never panic the interpreter (an internal
//! error `X999`). The full sweep is slow: `cargo test --release --test builtin_fuzz -- --ignored`;
//! the normal test runs a fixed sample of it.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

const ARRAY_METHODS: &[&str] = &[
    "all", "any", "chunk", "clear", "concat", "contains", "drop", "each", "filter", "find",
    "first", "flatten", "index_of", "insert", "is_empty", "join", "last", "len", "map", "max",
    "min", "pop", "push", "reduce", "remove", "repeat", "reverse", "slice", "sort", "sort_by",
    "sum", "swap", "take", "unique",
];
const MAP_METHODS: &[&str] = &[
    "clear",
    "filter",
    "get",
    "has",
    "is_empty",
    "keys",
    "len",
    "map_values",
    "merge",
    "remove",
    "values",
];
const STRING_METHODS: &[&str] = &[
    "camel_case",
    "capitalize",
    "chars",
    "code",
    "contains",
    "count",
    "ends_with",
    "eq_ignore_case",
    "index_of",
    "is_alpha",
    "is_blank",
    "is_digit",
    "is_empty",
    "kebab_case",
    "last_index_of",
    "len",
    "lines",
    "lower",
    "pad_left",
    "pad_right",
    "repeat",
    "replace",
    "reverse",
    "snake_case",
    "split",
    "starts_with",
    "strip_prefix",
    "strip_suffix",
    "substring",
    "title_case",
    "to_float",
    "to_int",
    "trim",
    "trim_end",
    "trim_start",
    "upper",
    "words",
];
const INT_METHODS: &[&str] = &[
    "abs",
    "acos",
    "asin",
    "atan",
    "atan2",
    "char",
    "clamp",
    "cos",
    "exp",
    "ln",
    "log",
    "log10",
    "log2",
    "max",
    "min",
    "pow",
    "signum",
    "sin",
    "sqrt",
    "tan",
    "to_bin",
    "to_degrees",
    "to_hex",
    "to_oct",
    "to_radians",
];
const FLOAT_METHODS: &[&str] = &[
    "abs",
    "acos",
    "asin",
    "atan",
    "atan2",
    "ceil",
    "clamp",
    "cos",
    "exp",
    "floor",
    "is_infinite",
    "is_nan",
    "ln",
    "log",
    "log10",
    "log2",
    "max",
    "min",
    "pow",
    "round",
    "signum",
    "sin",
    "sqrt",
    "tan",
    "to_degrees",
    "to_fixed",
    "to_radians",
    "trunc",
];
const DATETIME_METHODS: &[&str] = &[
    "add_days",
    "add_hours",
    "add_minutes",
    "add_months",
    "add_seconds",
    "add_years",
    "day",
    "days_until",
    "format",
    "hour",
    "minute",
    "month",
    "ms",
    "second",
    "start_of_day",
    "weekday",
    "year",
];

const RECEIVERS: &[(&str, &[&str])] = &[
    ("string", &["\"\"", "\"abc\"", "\"é𝒳é\"", "\"a,b,,c\""]),
    (
        "int",
        &[
            "0",
            "-1",
            "7",
            "9223372036854775807",
            "-9223372036854775807 - 1",
        ],
    ),
    (
        "float",
        &[
            "0.0",
            "-0.0",
            "1.5",
            "1e308",
            "float.NAN",
            "float.INFINITY",
            "-float.INFINITY",
        ],
    ),
    (
        "array",
        &[
            "[1, 2, 3]",
            "([] as array<int>)",
            "[9223372036854775807, 0]",
        ],
    ),
    (
        "map",
        &["{ \"a\": 1, \"b\": 2 }", "({} as map<string, int>)"],
    ),
    (
        "datetime",
        &["datetime.from_ms(0)", "datetime.from_ms(253402300799999)"],
    ),
];

const ARGS: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "3",
    "9223372036854775807",
    "-9223372036854775807 - 1",
    "1000000000000",
    "\"\"",
    "\"é\"",
    "\"𝒳𝒳\"",
    "\"a\"",
    "\"%\"",
    "\"(\"",
    "0.0",
    "-1.5",
    "1e308",
    "float.NAN",
    "[1, 2]",
    "[\"a\"]",
    "true",
    "none",
    "fun(x: int) -> int\n  x\nend",
    "fun(a: int, b: int) -> int\n  a - b\nend",
    "fun(x: string) -> bool\n  true\nend",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn methods_of(kind: &str) -> &'static [&'static str] {
    match kind {
        "string" => STRING_METHODS,
        "int" => INT_METHODS,
        "float" => FLOAT_METHODS,
        "array" => ARRAY_METHODS,
        "map" => MAP_METHODS,
        _ => DATETIME_METHODS,
    }
}

/// An internal error, a panic or a hang: what must never happen.
fn bad(src: &str) -> Option<String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = compile("main.q", &src, &mut host).ok()?;
            let limits = Limits {
                max_steps: 200_000,
                max_alloc: 100_000,
                ..Limits::default()
            };
            let r = prog.run(&mut host, limits);
            let d = r.error?;
            d.code
                .starts_with('X')
                .then(|| format!("{}: {}\n{src}", d.code, d.message))
        })
        .unwrap()
        .join()
        .unwrap_or_else(|_| Some("thread panicked".to_string()))
}

fn programs(sample: Option<usize>) -> Vec<String> {
    let mut all = Vec::new();
    for (kind, receivers) in RECEIVERS {
        for recv in *receivers {
            for m in methods_of(kind) {
                all.push(format!("let r = {recv}.{m}()\n"));
                for a in ARGS {
                    all.push(format!("let r = {recv}.{m}({a})\n"));
                }
                // two arguments: a smaller grid
                for a in ARGS.iter().step_by(3) {
                    for b in ARGS.iter().step_by(4) {
                        all.push(format!("let r = {recv}.{m}({a}, {b})\n"));
                    }
                }
            }
        }
    }
    match sample {
        None => all,
        Some(n) => {
            let mut rng = Rng(0x9E3779B97F4A7C15);
            (0..n)
                .map(|_| all[(rng.next() % all.len() as u64) as usize].clone())
                .collect()
        }
    }
}

#[test]
fn a_sample_of_builtin_calls_never_panics() {
    let failures: Vec<String> = programs(Some(1500)).iter().filter_map(|p| bad(p)).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

#[test]
#[ignore]
fn every_builtin_call_never_panics() {
    let failures: Vec<String> = programs(None).iter().filter_map(|p| bad(p)).collect();
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}
