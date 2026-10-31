//! `qlang fmt --stdin` and `[fmt] exclude`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-fmtcfg-{}-{name}", std::process::id()));
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

const UGLY: &str = "if true then\nprint(1)\nend\n";
const NEAT: &str = "if true then\n  print(1)\nend\n";

#[test]
fn stdin_is_laid_out_on_the_output() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["fmt", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(UGLY.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), NEAT);
}

#[test]
fn excluded_paths_are_left_alone() {
    let dir = project(
        "exclude",
        &[
            (
                "qlang.toml",
                "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[fmt]\nexclude = [\"vendor\", \"gen.q\"]\n",
            ),
            ("main.q", UGLY),
            ("gen.q", UGLY),
            ("vendor/lib.q", UGLY),
        ],
    );
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["fmt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(dir.join("main.q")).unwrap(), NEAT);
    assert_eq!(std::fs::read_to_string(dir.join("gen.q")).unwrap(), UGLY);
    assert_eq!(
        std::fs::read_to_string(dir.join("vendor/lib.q")).unwrap(),
        UGLY
    );
}
