//! `expr?`: the value of a `Result` or nullable, or leave the function with the error / `none`.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost {
                fs: Some(
                    [("data.json".to_string(), "{\"n\": 41}".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..MemoryHost::default()
            };
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return format!(
                        "COMPILE: {}",
                        e.diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.code, d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

const DIVIDE: &str = "fun divide(a: int, b: int) -> Result<int, Error>\n  if b == 0 then\n    return Err(Error { message: \"division by zero\" })\n  end\n  Ok(a div b)\nend\n";

#[test]
fn ok_gives_the_value_and_err_leaves_the_function() {
    let out = exec(&format!(
        "{DIVIDE}fun twice(a: int, b: int) -> Result<int, Error>\n  let half = divide(a, b)?\n  print(\"after\")\n  Ok(half * 2)\nend\nprint(twice(10, 2).unwrap())\nlet bad = twice(1, 0)\nprint(bad.is_err())\nprint(bad.unwrap_err().message)"
    ));
    assert_eq!(out, "after\n10\ntrue\ndivision by zero\n");
}

#[test]
fn question_marks_chain_and_work_in_expressions() {
    let out = exec(&format!(
        "{DIVIDE}fun f(a: int, b: int, c: int) -> Result<int, Error>\n  Ok(divide(a, b)? + divide(a, c)? * 10)\nend\nprint(f(10, 2, 5).unwrap())\nprint(f(10, 0, 5).is_err())\nprint(f(10, 2, 0).is_err())"
    ));
    assert_eq!(out, "25\ntrue\ntrue\n");
}

#[test]
fn it_leaves_from_inside_loops_and_blocks() {
    let out = exec(&format!(
        "{DIVIDE}fun sum(xs: array<int>) -> Result<int, Error>\n  let total = 0\n  for x in xs do\n    if x > 0 then\n      total += divide(100, x)?\n    end\n  end\n  Ok(total)\nend\nprint(sum([1, 2]).unwrap())\nprint(sum([1, 0]).is_err())"
    ));
    assert_eq!(out, "150\nfalse\n");
}

#[test]
fn nullable_values_leave_with_none() {
    let out = exec(
        "fun port(s: string) -> int?\n  let rest = s.strip_prefix(\"port=\")?\n  int.parse(rest)\nend\nprint(port(\"port=80\"))\nprint(port(\"host=1\"))\nprint(port(\"port=x\"))",
    );
    assert_eq!(out, "80\nnone\nnone\n");
}

#[test]
fn it_works_with_host_results() {
    let out = exec(
        "fun n() -> Result<int, Error>\n  let text = fs.read(\"data.json\")?\n  let json = Json.parse(text)?\n  let v = json.get(\"n\")\n  if v == none then\n    return Err(Error { message: \"no n\" })\n  end\n  Ok(v.as_number() as int)\nend\nprint(n().unwrap())\nfun missing() -> Result<string, Error>\n  Ok(fs.read(\"nope.json\")?)\nend\nprint(missing().is_err())",
    );
    assert_eq!(out, "41\ntrue\n");
}

#[test]
fn a_lambda_leaves_only_itself() {
    let out = exec(&format!(
        "{DIVIDE}let safe = fun(a: int) -> Result<int, Error>\n  Ok(divide(100, a)? + 1)\nend\nprint(safe(5).unwrap())\nprint(safe(0).is_err())\nprint(\"done\")"
    ));
    assert_eq!(out, "21\ntrue\ndone\n");
}

#[test]
fn the_function_must_be_able_to_return_what_question_mark_gives_back() {
    let out = exec(&format!("{DIVIDE}fun f() -> int\n  divide(1, 0)?\nend"));
    assert!(out.contains("T282"), "{out}");
    let out = exec(&format!("{DIVIDE}fun f()\n  divide(1, 0)?\nend"));
    assert!(out.contains("T282"), "{out}");
    let out = exec("fun g(s: string) -> int\n  s.strip_prefix(\"a\")?\n  1\nend");
    assert!(out.contains("T282"), "{out}");
}

#[test]
fn the_error_types_must_match() {
    let out = exec(&format!(
        "{DIVIDE}fun f() -> Result<int, string>\n  Ok(divide(1, 0)?)\nend"
    ));
    assert!(
        out.contains("T282") && out.contains("Result<_, Error>"),
        "{out}"
    );
}

#[test]
fn it_needs_a_function_and_a_result_or_nullable() {
    let out = exec(&format!("{DIVIDE}let x = divide(1, 2)?"));
    assert!(out.contains("T280"), "{out}");
    let out = exec("fun f() -> Result<int, Error>\n  let n = 5\n  Ok(n?)\nend");
    assert!(out.contains("T281"), "{out}");
}

#[test]
fn the_question_mark_in_types_is_unchanged() {
    let out = exec(
        "let x: int? = 5\nlet y = x as int?\nfun f(a: string?) -> string?\n  a\nend\nprint(f(\"a\"))",
    );
    assert_eq!(out, "a\n");
}
