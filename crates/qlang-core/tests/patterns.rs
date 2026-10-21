//! Patterns that nest: variants inside tuples, patterns inside variants, and the coverage check.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
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

const SHAPES: &str = "enum Shape\n  Circle(r: int)\n  Rect(w: int, h: int)\n  Empty\nend\nenum Color\n  Red\n  Green\nend\n";

#[test]
fn a_variant_inside_a_tuple_pattern() {
    let src = format!(
        "{SHAPES}fun f(c: Color, n: int) -> string\n  match (c, n)\n    case (Color.Red, 0) then \"red zero\"\n    case (Red, x) then \"red {{x}}\"\n    case (Green, _) then \"green\"\n  end\nend\nprint(f(Color.Red, 0))\nprint(f(Color.Red, 5))\nprint(f(Color.Green, 1))"
    );
    assert_eq!(exec(&src), "red zero\nred 5\ngreen\n");
}

#[test]
fn a_variant_with_data_inside_a_tuple_pattern() {
    let src = format!(
        "{SHAPES}let s = (Shape.Circle(2), Shape.Rect(3, 4))\nmatch s\n  case (Circle(r), Rect(w, h)) then print(r + w + h)\n  else print(0)\nend"
    );
    assert_eq!(exec(&src), "9\n");
}

#[test]
fn patterns_nest_inside_a_variant() {
    let src = "let r: Result<(int, string), Error> = Ok((7, \"x\"))\nmatch r\n  case Ok((n, s)) then print(\"{n}{s}\")\n  case Err(e) then print(e.message)\nend";
    assert_eq!(exec(src), "7x\n");
    let src = "let r: Result<int?, Error> = Ok(none)\nmatch r\n  case Ok(none) then print(\"nothing\")\n  case Ok(n) then print(n)\n  case Err(_) then print(\"err\")\nend";
    assert_eq!(exec(src), "nothing\n");
    let src = "let inner: Result<int, Error> = Ok(3)\nlet r: Result<Result<int, Error>, Error> = Ok(inner)\nmatch r\n  case Ok(Ok(n)) then print(n)\n  case Ok(Err(e)) then print(e.message)\n  case Err(e) then print(e.message)\nend";
    assert_eq!(exec(src), "3\n");
    let src = "let r: Result<int, Error> = Ok(4)\nmatch r\n  case Ok(0) then print(\"zero\")\n  case Ok(1..=9) then print(\"digit\")\n  case Ok(_) then print(\"big\")\n  case Err(_) then print(\"err\")\nend";
    assert_eq!(exec(src), "digit\n");
}

#[test]
fn a_name_that_is_a_variant_inside_a_pattern_is_the_variant() {
    let src = format!(
        "{SHAPES}match (Shape.Empty, 1)\n  case (Empty, n) then print(\"empty {{n}}\")\n  else print(\"other\")\nend\nmatch (Shape.Circle(1), 1)\n  case (Empty, n) then print(\"empty {{n}}\")\n  else print(\"other\")\nend"
    );
    assert_eq!(exec(&src), "empty 1\nother\n");
}

#[test]
fn coverage_is_checked_through_nesting() {
    let ok = format!(
        "{SHAPES}fun f(a: Color, b: Color) -> int\n  match (a, b)\n    case (Red, Red) then 1\n    case (Red, Green) then 2\n    case (Green, _) then 3\n  end\nend\nprint(f(Color.Green, Color.Red))"
    );
    assert_eq!(exec(&ok), "3\n");
    let missing = format!(
        "{SHAPES}fun f(a: Color, b: Color) -> int\n  match (a, b)\n    case (Red, Red) then 1\n    case (Green, _) then 3\n  end\nend\nprint(f(Color.Green, Color.Red))"
    );
    assert!(exec(&missing).starts_with("COMPILE"), "{}", exec(&missing));
    let result_missing = "fun f(r: Result<int, Error>) -> int\n  match r\n    case Ok(0) then 0\n    case Err(_) then 1\n  end\nend\nprint(f(Ok(1)))";
    assert!(
        exec(result_missing).starts_with("COMPILE"),
        "{}",
        exec(result_missing)
    );
    let nullable = "fun f(a: int?, b: bool) -> int\n  match (a, b)\n    case (none, _) then 0\n    case (_, true) then 1\n    case (_, false) then 2\n  end\nend\nprint(f(3, false))";
    assert_eq!(exec(nullable), "2\n");
    let guarded = format!(
        "{SHAPES}fun f(c: Color) -> int\n  match c\n    case Red then 1\n    case Green if false then 2\n  end\nend\nprint(f(Color.Red))"
    );
    assert!(exec(&guarded).starts_with("COMPILE"), "{}", exec(&guarded));
}

#[test]
fn a_pattern_in_a_variant_must_fit() {
    let src = "let r: Result<int, Error> = Ok(1)\nmatch r\n  case Ok(\"a\") then print(1)\n  else print(0)\nend";
    assert!(exec(src).contains("T220"), "{}", exec(src));
    let src = "let r: Result<int, Error> = Ok(1)\nmatch r\n  case Ok(a, b) then print(1)\n  else print(0)\nend";
    assert!(exec(src).contains("T222"), "{}", exec(src));
}
