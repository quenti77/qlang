//! Mutated programs must never make the compiler or the interpreter panic.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(src: &str, rng: &mut Rng, count: usize) -> String {
    let mut chars: Vec<char> = src.chars().collect();
    let words = ["end", "then", "fun", "(", ")", "[", "]", "{", "}", "\"", "--(", "--\"", "none", "let", "x", "1", "2.5", "..", "=", "+", "\n", "struct", "impl", "match", "case", "else", "return", ".", ",", ":", "->", "?", "<", ">", "{x}", "self", "super", "import", "export"];
    for _ in 0..count {
        if chars.is_empty() {
            break;
        }
        match rng.below(6) {
            0 => {
                let i = rng.below(chars.len());
                let n = 1 + rng.below(12);
                chars.drain(i..(i + n).min(chars.len()));
            }
            1 => {
                let i = rng.below(chars.len());
                let w: Vec<char> = words[rng.below(words.len())].chars().collect();
                for (k, c) in w.into_iter().enumerate() {
                    chars.insert(i + k, c);
                }
            }
            2 => {
                let i = rng.below(chars.len());
                let n = (1 + rng.below(40)).min(chars.len() - i);
                let slice: Vec<char> = chars[i..i + n].to_vec();
                let j = rng.below(chars.len());
                for (k, c) in slice.into_iter().enumerate() {
                    chars.insert(j + k, c);
                }
            }
            3 => {
                let i = rng.below(chars.len());
                chars[i] = ['x', ' ', '\n', '"', '(', ')', 'é', '0', '-'][rng.below(9)];
            }
            4 => {
                // replace one word by another word of the program
                let text: String = chars.iter().collect();
                let ws: Vec<&str> = text.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
                if ws.len() > 2 {
                    let (a, b) = (ws[rng.below(ws.len())], ws[rng.below(ws.len())]);
                    let replaced = text.replacen(a, b, 1);
                    chars = replaced.chars().collect();
                }
            }
            _ => {
                if rng.below(8) == 0 {
                    let i = rng.below(chars.len());
                    chars.truncate(i);
                }
            }
        }
    }
    chars.into_iter().collect()
}

#[test]
fn mutated_programs_do_not_panic() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let seeds: Vec<String> = ["tour.q", "syntax.q", "tour_math.q"]
        .iter()
        .map(|f| std::fs::read_to_string(format!("{dir}/{f}")).unwrap())
        .collect();
    let math = seeds[2].clone();
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            let mut rng = Rng(0x9E3779B97F4A7C15);
            let mut compiled = 0;
            let mut total = 0;
            for round in 0..20000 {
                let seed = &seeds[rng.below(2)];
                let count = if round % 3 == 0 { 3 } else { 1 };
                let src = mutate(seed, &mut rng, count);
                total += 1;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut host = MemoryHost::new();
                    host.files.insert("tour_math.q".into(), math.clone());
                    host.allow_read = true;
                    match compile("main.q", &src, &mut host) {
                        Ok(p) => {
                            let limits = Limits { max_steps: 20_000, max_depth: 200, max_output: 100_000, max_alloc: 100_000 };
                            let _ = p.run(&mut host, limits);
                            true
                        }
                        Err(e) => {
                            let _ = e.render();
                            false
                        }
                    }
                }));
                match result {
                    Ok(true) => compiled += 1,
                    Ok(false) => {}
                    Err(_) => {
                        std::fs::write(std::env::temp_dir().join("qlang_fuzz_crash.q"), &src).ok();
                        panic!("panic on round {round}; input saved to qlang_fuzz_crash.q:\n{src}");
                    }
                }
            }
            println!("{compiled}/{total} mutated programs still compiled");
        })
        .unwrap()
        .join()
        .unwrap();
}
