//! `Term.script`: a terminal played by a test.

use std::process::Command;

const PROGRAM: &str = r#"
let r = Term.script([Event { kind: "key", key: "a" }, Event { kind: "key", key: "Up", shift: true }, Event { kind: "resize", x: 100, y: 40 }, Event { kind: "key", key: "q" }], 30, 10)
print(r.is_ok())
print(Term.is_tty())
let s = Term.size()
print("{s.cols}x{s.rows}")
Term.session(fun() -> bool
  Term.frame("hello")
  let seen: array<string> = []
  while true do
    let e = Term.event(1000).unwrap()
    if e.kind == "timeout" then
      break
    end
    let mark = if e.shift then "S" else "-" end
    seen.push(e.kind + ":" + e.key + ":" + mark + ":{e.x}")
    if e.kind == "key" and e.key == "q" then
      break
    end
  end
  print(seen)
  true
end)
print(Term.screen().unwrap().contains("hello"))
print(Term.event(5000).unwrap().kind)
print(Term.end_script())
print(Term.screen().is_err())
"#;

#[test]
fn a_program_reads_scripted_events_and_its_screen_is_kept() {
    let dir = std::env::temp_dir().join(format!("qlang-termscript-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.q"), PROGRAM).unwrap();
    let started = std::time::Instant::now();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "main.q"])
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{}{out}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        out,
        "true\ntrue\n30x10\n[\"key:a:-:0\", \"key:Up:S:0\", \"resize::-:100\", \"key:q:-:0\"]\ntrue\ntimeout\ntrue\ntrue\n"
    );
    // the last `Term.event(5000)` found nothing and did not wait for it
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
}
