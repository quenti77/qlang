//! `[stability]` in `qlang.toml`, shown by `qlang doc`.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-levels-{}-{name}", std::process::id()));
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn doc(dir: &PathBuf, args: &[&str]) -> (String, String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("doc")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

const MODULE: &str = "--\" Adds.\n--\"\nfun add(a: int, b: int) -> int\n  a + b\nend\n";
const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[stability]\nstable = [\"core.q\"]\nprovisional = [\"web/*.q\"]\nexperimental = [\"web/ws.q\"]\n";

#[test]
fn each_file_says_its_level_and_an_exact_name_beats_a_pattern() {
    let dir = project(
        "levels",
        &[
            ("qlang.toml", MANIFEST),
            ("core.q", MODULE),
            ("web/app.q", MODULE),
            ("web/ws.q", MODULE),
        ],
    );
    let (out, err, code) = doc(&dir, &[]);
    assert_eq!(code, 0, "{err}");
    let level_after = |heading: &str| -> String {
        let i = out
            .find(heading)
            .unwrap_or_else(|| panic!("no {heading} in {out}"));
        out[i..].lines().nth(2).unwrap().to_string()
    };
    assert_eq!(level_after("# core.q"), "_Stability: **stable**._");
    assert_eq!(level_after("# web/app.q"), "_Stability: **provisional**._");
    assert_eq!(level_after("# web/ws.q"), "_Stability: **experimental**._");
}

#[test]
fn require_levels_fails_for_a_file_without_one() {
    let dir = project(
        "missing",
        &[
            ("qlang.toml", MANIFEST),
            ("core.q", MODULE),
            ("extra.q", MODULE),
        ],
    );
    let (_, err, code) = doc(&dir, &["--require-levels"]);
    assert_eq!(code, 1);
    assert!(err.contains("extra.q: no stability level"), "{err}");
    // without the flag it is only documentation
    assert_eq!(doc(&dir, &[]).2, 0);
}
