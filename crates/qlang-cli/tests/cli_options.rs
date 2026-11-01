//! `--max-steps` and `--seed` on `qlang run` and `qlang test`: part of what the 1.0 promises for these commands.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-options-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        fs::write(self.0.join(rel), text).unwrap();
    }

    fn qlang(&self, args: &[&str]) -> (String, String, Option<i32>) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
            o.status.code(),
        )
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const LOOP: &str = "let n = 0\nwhile n < 1000000 do\n  n += 1\nend\nprint(n)\n";
const DRAW: &str =
    "print(random.int(0, 1000000))\nprint(random.int(0, 1000000))\nprint(random.int(0, 1000000))\n";

#[test]
fn run_stops_a_program_that_goes_past_max_steps() {
    let d = Dir::new("run-steps");
    d.write("loop.q", LOOP);
    let (out, err, code) = d.qlang(&["run", "loop.q", "--max-steps", "1000"]);
    assert!(err.contains("R900"), "{err}");
    assert_eq!((out.as_str(), code), ("", Some(2)));
    // without the limit, or with a big enough one, the same program finishes
    let (out, _, code) = d.qlang(&["run", "loop.q", "--max-steps", "100000000"]);
    assert_eq!((out.as_str(), code), ("1000000\n", Some(0)));
}

#[test]
fn test_applies_max_steps_to_each_test() {
    let d = Dir::new("test-steps");
    d.write("qlang.toml", "[package]\nname = \"t\"\n");
    d.write(
        "slow_test.q",
        "fun test_fast()\n  assert_eq(1 + 1, 2)\nend\n\nfun test_slow()\n  let n = 0\n  while n < 1000000 do\n    n += 1\n  end\nend\n",
    );
    let (out, err, code) = d.qlang(&["test", "--max-steps", "1000"]);
    assert_eq!(code, Some(1), "{out}{err}");
    assert!(out.contains("ok   slow_test.q::test_fast"), "{out}");
    assert!(out.contains("FAIL slow_test.q::test_slow"), "{out}");
    assert!(out.contains("R900"), "{out}");
    let (out, err, code) = d.qlang(&["test", "--max-steps", "100000000"]);
    assert_eq!(code, Some(0), "{out}{err}");
}

#[test]
fn max_steps_needs_a_number() {
    let d = Dir::new("steps-arg");
    d.write("loop.q", LOOP);
    let (_, _, code) = d.qlang(&["run", "loop.q", "--max-steps", "many"]);
    assert_eq!(code, Some(64));
}

#[test]
fn the_same_seed_gives_the_same_random_numbers_and_another_seed_other_ones() {
    let d = Dir::new("seed-run");
    d.write("draw.q", DRAW);
    let (a, _, ca) = d.qlang(&["run", "draw.q", "--seed", "7"]);
    let (b, _, cb) = d.qlang(&["run", "draw.q", "--seed", "7"]);
    let (c, _, _) = d.qlang(&["run", "draw.q", "--seed", "8"]);
    assert_eq!((ca, cb), (Some(0), Some(0)));
    assert_eq!(a.lines().count(), 3, "{a}");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn test_with_a_seed_replays_the_same_numbers() {
    let d = Dir::new("seed-test");
    d.write("qlang.toml", "[package]\nname = \"t\"\n");
    d.write(
        "seed_test.q",
        "fun test_prints_a_draw()\n  print(\"drew {random.int(0, 1000000)}\")\nend\n",
    );
    let drew = |seed: &str| -> String {
        let (out, err, code) = d.qlang(&["test", "--seed", seed]);
        assert_eq!(code, Some(0), "{out}{err}");
        out.lines()
            .find(|l| l.contains("drew"))
            .unwrap_or_else(|| panic!("no draw in {out}"))
            .to_string()
    };
    assert_eq!(drew("3"), drew("3"));
    assert_ne!(drew("3"), drew("4"));
}
