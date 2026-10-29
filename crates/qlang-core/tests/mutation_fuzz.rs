//! Damaged programs must be refused with diagnostics, never panic the compiler or the formatter.
//! Every program of `examples/` is mutated (cut, duplicated, swapped, filled with syntax) from a
//! fixed seed, so a failure can be replayed. `QLANG_FUZZ_ROUNDS=20000 cargo test --release
//! --test mutation_fuzz` runs a long sweep, with `QLANG_FUZZ_SEED` to change the seed.

use qlang_core::host::MemoryHost;
use qlang_core::{check::compile, fmt, lexer, parser};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const PIECES: &[&str] = &[
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "\"",
    "'",
    "\\",
    "end",
    "fun ",
    "struct ",
    "enum ",
    "match ",
    "if ",
    "then",
    "else",
    "for ",
    "in ",
    "while ",
    "do",
    "let ",
    "return",
    "import ",
    "#[",
    "->",
    "=>",
    "..",
    "?",
    "!",
    "<",
    ">",
    "::",
    ":",
    ",",
    ".",
    "=",
    "==",
    "\n",
    "\t",
    "\r\n",
    "0",
    "-1",
    "9999999999999999999999",
    "1e999",
    "\"{",
    "}\"",
    "\u{0}",
    "é",
    "😀",
    "\u{202e}",
    "-- ",
    "---",
    "public ",
    "trait ",
    "impl ",
    "extern fun ",
    "await ",
    "or ",
    "and ",
    "not ",
    "none",
    "T",
];

fn mutate(src: &str, rng: &mut Rng) -> String {
    let chars: Vec<char> = src.chars().collect();
    let len = chars.len();
    let mut out = chars.clone();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(out.len() + 1);
        match rng.below(6) {
            0 => {
                // cut a range
                let to = (at + rng.below(40)).min(out.len());
                out.drain(at.min(out.len())..to);
            }
            1 => {
                // duplicate a range
                let to = (at + rng.below(60)).min(out.len());
                let piece: Vec<char> = out[at.min(out.len())..to].to_vec();
                for (i, c) in piece.into_iter().enumerate() {
                    out.insert(at + i, c);
                }
            }
            2 => {
                // insert a piece of syntax
                let piece = PIECES[rng.below(PIECES.len())];
                for (i, c) in piece.chars().enumerate() {
                    out.insert((at + i).min(out.len()), c);
                }
            }
            3 => {
                // replace a character by a piece of syntax
                if at < out.len() {
                    out.remove(at);
                    let piece = PIECES[rng.below(PIECES.len())];
                    for (i, c) in piece.chars().enumerate() {
                        out.insert((at + i).min(out.len()), c);
                    }
                }
            }
            4 => out.truncate(at),
            _ => {
                // swap two characters far apart
                if len > 2 && !out.is_empty() {
                    let (a, b) = (rng.below(out.len()), rng.below(out.len()));
                    out.swap(a, b);
                }
            }
        }
    }
    out.into_iter().collect()
}

/// What the compiler, the parser and the formatter do with `src`; `Some` when one panicked.
fn crash(src: &str) -> Option<String> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mut diags = Vec::new();
        let _ = lexer::lex(src, 0, 0, &mut diags);
        let mut diags = Vec::new();
        let _ = parser::parse_module(src, 0, &mut diags);
        let mut host = MemoryHost::default();
        let _ = compile("main.q", src, &mut host);
        let _ = fmt::format(src);
    }));
    outcome.err().map(|e| {
        e.downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "panic".to_string())
    })
}

fn seeds() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "q")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push((
                path.file_name().unwrap().to_string_lossy().to_string(),
                text,
            ));
        }
    }
    out.sort();
    out
}

fn sweep(rounds: usize, seed: u64) {
    let seeds = seeds();
    assert!(!seeds.is_empty(), "no example program found");
    let failures = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let mut rng = Rng(seed);
            let mut failures = Vec::new();
            for round in 0..rounds {
                let (name, text) = &seeds[round % seeds.len()];
                let mutant = mutate(text, &mut rng);
                if let Some(why) = crash(&mutant) {
                    failures.push(format!(
                        "{name} (round {round}, seed {seed}): {why}\n{mutant}"
                    ));
                    if failures.len() >= 3 {
                        break;
                    }
                }
            }
            failures
        })
        .unwrap()
        .join()
        .expect("the sweep itself crashed");
    assert!(failures.is_empty(), "{}", failures.join("\n=====\n"));
}

#[test]
fn damaged_examples_never_panic() {
    let rounds = std::env::var("QLANG_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let seed = std::env::var("QLANG_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x9E3779B97F4A7C15);
    sweep(rounds, seed);
}

#[test]
fn nasty_inputs_never_panic() {
    for src in [
        "",
        "\0",
        "\"",
        "\"{",
        "{{{{{{{{{{",
        "((((((((((",
        "fun",
        "fun f(",
        "struct S {",
        "end end end",
        "let x = 1e999999",
        "let x = 99999999999999999999999999",
        "'",
        "#[",
        "---",
        "\u{202e}",
        "let s = \"\\u{110000}\"",
        "match",
        "import",
        "a.b.c.d.e.f.g.h",
        "[[[[[[[[[[[[[[[[[[[[[[[[",
    ] {
        assert_eq!(crash(src), None, "{src:?}");
    }
    let deep = format!("let x = {}1{}", "(".repeat(5000), ")".repeat(5000));
    let source = deep.clone();
    let result = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || crash(&source))
        .unwrap()
        .join();
    assert!(matches!(result, Ok(None)), "deep nesting: {result:?}");
}
