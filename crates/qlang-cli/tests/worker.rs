//! `Worker`: a file run at the same time as the program, in an interpreter of its own.

use std::path::PathBuf;
use std::process::Command;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-worker-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn run(dir: &PathBuf, extra: &[&str]) -> (String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg("main.q")
        .args(extra)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

const SQUARE: &str = "fun run()\n  let n = (Worker.input().get(\"n\") ?? Json.null()).as_number()\n  Worker.finish(Json.number(n * n))\nend\n";

const ECHO: &str = "fun run()\n  let count = 0\n  while not Worker.cancelled() do\n    let m = Worker.next(200)\n    if m != none then\n      count += 1\n      Worker.emit(Json.of(\"echo {m.to_text()}\"))\n    end\n  end\n  Worker.finish(Json.number(count as float))\nend\n";

#[test]
fn a_task_computes_and_gives_its_result() {
    let dir = project(
        "result",
        &[
            ("square.q", SQUARE),
            (
                "main.q",
                "let w = Worker.spawn(\"square.q\", Json.of({\"n\": 12})).unwrap()\nprint(assert_some(w.wait(5000).unwrap()).to_text())\n",
            ),
        ],
    );
    let (out, err) = run(&dir, &[]);
    assert_eq!(out.trim(), "144", "{err}");
}

#[test]
fn the_program_and_the_task_talk_and_the_task_can_be_asked_to_stop() {
    let dir = project(
        "talk",
        &[
            ("echo.q", ECHO),
            (
                "main.q",
                r#"
let e = Worker.spawn("echo.q", Json.null()).unwrap()
e.send(Json.of("a")).unwrap()
e.send(Json.of("b")).unwrap()
print(assert_some(e.recv(3000).unwrap()).to_text())
print(assert_some(e.recv(3000).unwrap()).to_text())
print(e.recv(100).unwrap() == none)
print(e.wait(50).unwrap() == none)
e.cancel()
print(assert_some(e.wait(3000).unwrap()).to_text())
"#,
            ),
        ],
    );
    let (out, err) = run(&dir, &[]);
    assert_eq!(
        out, "\"echo \\\"a\\\"\"\n\"echo \\\"b\\\"\"\ntrue\ntrue\n2\n",
        "{err}"
    );
}

#[test]
fn cancel_stops_a_loop_that_never_looks() {
    let dir = project(
        "spin",
        &[
            (
                "spin.q",
                "fun run()\n  let i = 0\n  while true do\n    i += 1\n  end\nend\n",
            ),
            (
                "main.q",
                r#"
let s = Worker.spawn("spin.q", Json.null()).unwrap()
print(s.wait(200).unwrap() == none)
print(s.is_done())
s.cancel()
let r = s.wait(5000)
print(r.is_err())
print(r.unwrap_err().message.contains("R906"))
"#,
            ),
        ],
    );
    let started = std::time::Instant::now();
    let (out, err) = run(&dir, &["--max-steps", "100000000000"]);
    assert_eq!(out, "true\nfalse\ntrue\ntrue\n", "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn a_task_that_fails_or_is_missing_is_an_error_with_the_reason() {
    let dir = project(
        "fail",
        &[
            (
                "bad.q",
                "fun run()\n  let r: Result<int, Error> = Result<int, Error>.Err(Error { message: \"boom\" })\n  r.unwrap()\nend\n",
            ),
            ("norun.q", "let x = 1\n"),
            ("broken.q", "fun run(\n"),
            (
                "main.q",
                r#"
let b = Worker.spawn("bad.q", Json.null()).unwrap().wait(3000)
print(b.is_err())
print(b.unwrap_err().message.contains("the task failed"))
let n = Worker.spawn("norun.q", Json.null()).unwrap().wait(3000)
print(n.unwrap_err().message.contains("has no `fun run()`"))
let k = Worker.spawn("broken.q", Json.null()).unwrap().wait(3000)
print(k.is_err())
print(Worker.spawn("missing.q", Json.null()).is_err())
let w = Worker.spawn("bad.q", Json.null()).unwrap()
w.wait(3000)
print(w.wait(100).is_err())
"#,
            ),
        ],
    );
    let (out, err) = run(&dir, &[]);
    assert_eq!(out, "true\ntrue\ntrue\ntrue\ntrue\ntrue\n", "{err}");
}

#[test]
fn a_task_cannot_be_a_file_outside_of_root() {
    let base = project(
        "root",
        &[(
            "main.q",
            "let r = Worker.spawn(\"../outside.q\", Json.null())\nprint(r.is_err())\nprint(r.unwrap_err().message.contains(\"outside of --root\"))\n",
        )],
    );
    std::fs::write(base.parent().unwrap().join("outside.q"), SQUARE).unwrap();
    let (out, err) = run(&base, &["--root", base.to_str().unwrap()]);
    assert_eq!(out, "true\ntrue\n", "{err}");
}

#[test]
fn only_so_many_tasks_run_at_once() {
    let dir = project(
        "limit",
        &[
            ("idle.q", "fun run()\n  Worker.next(30000)\nend\n"),
            (
                "main.q",
                r#"
let refused = 0
let started: array<Worker> = []
for i in 0..70 do
  let w = Worker.spawn("idle.q", Json.null())
  if w.is_err() then
    refused += 1
  else
    started.push(w.unwrap())
  end
end
print(refused >= 6)
for w in started do
  w.cancel()
end
print(started.len() <= 64)
"#,
            ),
        ],
    );
    let (out, err) = run(&dir, &[]);
    assert_eq!(out, "true\ntrue\n", "{err}");
}
