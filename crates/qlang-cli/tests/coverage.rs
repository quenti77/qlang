//! `qlang test --coverage` and `--group`.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-cover-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn test(dir: &PathBuf, args: &[&str]) -> (String, i32) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("test")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        o.status.code().unwrap_or(-1),
    )
}

const LIB: &str = "fun sign(n: int) -> string\n  if n > 0 then\n    return \"pos\"\n  end\n  if n < 0 then\n    return \"neg\"\n  end\n  \"zero\"\nend\nfun unused() -> int\n  let a = 1\n  a + 1\nend\nexport sign, unused\n";
const SPEC: &str = "import sign from \"lib.q\"\nfun test_pos()\n  assert_eq(sign(3), \"pos\")\nend\nfun test_zero()\n  assert_eq(sign(0), \"zero\")\nend\n";

#[test]
fn coverage_names_the_lines_that_did_not_run() {
    let dir = project("lines", &[("lib.q", LIB), ("lib_test.q", SPEC)]);
    let (out, code) = test(&dir, &["--coverage", "--coverage-lcov", "c.info"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("lib.q (4/7)  not run: 6, 11, 12"), "{out}");
    assert!(
        !out.contains("lib_test.q ("),
        "the test file is not reported: {out}"
    );
    let lcov = std::fs::read_to_string(dir.join("c.info")).unwrap();
    assert!(lcov.contains("SF:lib.q\n"), "{lcov}");
    assert!(
        lcov.contains("DA:6,0\n") && lcov.contains("DA:5,1\n"),
        "{lcov}"
    );
    assert!(lcov.contains("LF:7\nLH:4\nend_of_record"), "{lcov}");
}

#[test]
fn without_the_flag_nothing_is_reported() {
    let dir = project("off", &[("lib.q", LIB), ("lib_test.q", SPEC)]);
    let (out, code) = test(&dir, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("coverage"), "{out}");
}

#[test]
fn group_and_skip_group_reach_the_tests_as_variables() {
    let spec = "fun test_env()\n  let g = env.get(\"QLANG_TEST_GROUP\") ?? \"-\"\n  let s = env.get(\"QLANG_TEST_SKIP_GROUP\") ?? \"-\"\n  print(\"G={g} S={s}\")\nend\n";
    let dir = project("group", &[("env_test.q", spec)]);
    let (out, _) = test(&dir, &["--group", "fast,db", "--skip-group", "slow"]);
    assert!(out.contains("G=fast,db S=slow"), "{out}");
}
