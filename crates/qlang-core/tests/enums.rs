//! Enumerations whose variants carry data, generic ones included.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// Output of the program, or `CODE: message` (compile or run error).
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

const SHAPE: &str =
    "enum Shape\n  Circle(radius: float)\n  Rect(w: float, h: float)\n  Empty\nend\n";

fn with_shape(rest: &str) -> String {
    format!("{SHAPE}{rest}")
}

#[test]
fn variants_carry_values_and_match_binds_them() {
    let out = exec(&with_shape(
        "fun area(s: Shape) -> float\n  match s\n    case Circle(r) then 3.0 * r * r\n    case Shape.Rect(w, h) then w * h\n    case Empty then 0.0\n  end\nend\nprint(area(Shape.Circle(2.0)))\nprint(area(Shape.Rect(w: 3.0, h: 4.0)))\nprint(area(Shape.Empty))",
    ));
    assert_eq!(out, "12.0\n12.0\n0.0\n");
}

#[test]
fn values_print_compare_and_report_their_class() {
    let out = exec(&with_shape(
        "let c = Shape.Circle(2.0)\nprint(c)\nprint(Shape.Rect(1.0, 2.0))\nprint(Shape.Empty)\nprint(c == Shape.Circle(2.0))\nprint(c == Shape.Circle(3.0))\nprint(c != Shape.Empty)\nprint(c.class())",
    ));
    assert_eq!(
        out,
        "Shape.Circle(2.0)\nShape.Rect(1.0, 2.0)\nShape.Empty\ntrue\nfalse\ntrue\nShape\n"
    );
}

#[test]
fn a_match_must_cover_every_variant() {
    let out = exec(&with_shape(
        "let s = Shape.Empty\nlet n = match s\n  case Circle(r) then 1\n  case Empty then 0\nend\nprint(n)",
    ));
    assert!(out.starts_with("COMPILE:"), "{out}");
    let ok = exec(&with_shape(
        "let s = Shape.Empty\nlet n = match s\n  case Circle(r) then 1\n  case Rect(w, h) then 2\n  else 0\nend\nprint(n)",
    ));
    assert_eq!(ok, "0\n");
}

#[test]
fn a_guard_does_not_count_as_covering_a_variant() {
    let out = exec(&with_shape(
        "let n = match Shape.Empty\n  case Circle(r) if r > 0.0 then 1\n  case Rect(w, h) then 2\n  case Empty then 3\nend\nprint(n)",
    ));
    assert!(out.starts_with("COMPILE:"), "{out}");
}

#[test]
fn patterns_are_checked_against_the_variant() {
    let wrong_count = exec(&with_shape(
        "match Shape.Empty\n  case Rect(w) then print(w)\n  else print(0)\nend",
    ));
    assert!(wrong_count.contains("T222"), "{wrong_count}");
    let no_parens = exec(&with_shape(
        "match Shape.Empty\n  case Circle then print(1)\n  else print(0)\nend",
    ));
    assert!(no_parens.contains("T222"), "{no_parens}");
    let unknown = exec(&with_shape(
        "match Shape.Empty\n  case Hexagon(x) then print(1)\n  else print(0)\nend",
    ));
    assert!(unknown.contains("T221"), "{unknown}");
}

#[test]
fn binding_types_come_from_the_variant() {
    let out = exec(&with_shape(
        "match Shape.Circle(1.0)\n  case Circle(r) then\n    let s: string = r\n  else print(0)\nend",
    ));
    assert!(out.contains("T00"), "{out}");
}

#[test]
fn construction_is_checked() {
    assert!(exec(&with_shape("let c = Shape.Circle(\"big\")")).contains("expected"));
    assert!(exec(&with_shape("let c = Shape.Circle()")).starts_with("COMPILE:"));
    assert!(exec(&with_shape("let c = Shape.Empty()")).contains("T164"));
    assert!(exec(&with_shape("let c = Shape.Circle")).contains("T164"));
    assert!(exec(&with_shape("let c = Shape.Rect(w: 1.0, 2.0)")).starts_with("COMPILE:"));
}

#[test]
fn underscore_ignores_a_value() {
    let out = exec(&with_shape(
        "match Shape.Rect(3.0, 4.0)\n  case Rect(_, h) then print(h)\n  else print(0)\nend",
    ));
    assert_eq!(out, "4.0\n");
}

#[test]
fn generic_enums_infer_their_arguments() {
    let out = exec(
        "enum Maybe<T>\n  Just(value: T)\n  Nothing\nend\n\nfun first(xs: array<int>) -> Maybe<int>\n  if xs.len() > 0 then Maybe.Just(xs[0]) else Maybe.Nothing end\nend\n\nfun show(m: Maybe<int>) -> string\n  match m\n    case Just(v) then \"got {v + 1}\"\n    case Nothing then \"none\"\n  end\nend\nprint(show(first([7, 8])))\nprint(show(first([])))",
    );
    assert_eq!(out, "got 8\nnone\n");
}

#[test]
fn methods_and_traits_work_on_enums_with_data() {
    let out = exec(
        "enum Shape\n  Circle(radius: float)\n  Square(side: float)\nend\n\ntrait Named\n  fun name(self) -> string\nend\n\nimpl Shape\n  public fun area(self) -> float\n    match self\n      case Circle(r) then 3.0 * r * r\n      case Square(s) then s * s\n    end\n  end\nend\n\nimpl Named for Shape\n  public fun name(self) -> string\n    match self\n      case Circle(_) then \"circle\"\n      case Square(_) then \"square\"\n    end\n  end\nend\n\nlet shapes = [Shape.Circle(1.0), Shape.Square(2.0)]\nfor s in shapes do\n  print(\"{s.name()} {s.area()}\")\nend",
    );
    assert_eq!(out, "circle 3.0\nsquare 4.0\n");
}

#[test]
fn generic_enums_can_have_generic_impls() {
    let out = exec(
        "enum Maybe<T>\n  Just(value: T)\n  Nothing\nend\n\nimpl<T> Maybe<T>\n  public fun is_just(self) -> bool\n    match self\n      case Just(_) then true\n      case Nothing then false\n    end\n  end\nend\n\nprint(Maybe.Just(1).is_just())\nlet n: Maybe<string> = Maybe.Nothing\nprint(n.is_just())",
    );
    assert_eq!(out, "true\nfalse\n");
}

#[test]
fn data_enums_cannot_be_map_keys_but_plain_ones_still_can() {
    let bad = exec(&with_shape("let m: map<Shape, int> = {}"));
    assert!(bad.contains("T270"), "{bad}");
    let ok = exec("enum Color\n  Red\n  Green\nend\nlet m = { Color.Red: 1 }\nprint(m[Color.Red])");
    assert_eq!(ok, "Some(1)\n".replace("Some(1)", "1"));
}

#[test]
fn variants_can_nest_and_hold_collections() {
    let out = exec(
        "enum Tree\n  Leaf(value: int)\n  Node(children: array<Tree>)\nend\n\nfun sum(t: Tree) -> int\n  match t\n    case Leaf(v) then v\n    case Node(cs) then\n      let total = 0\n      for c in cs do\n        total += sum(c)\n      end\n      total\n  end\nend\nprint(sum(Tree.Node([Tree.Leaf(1), Tree.Node([Tree.Leaf(2), Tree.Leaf(3)])])))",
    );
    assert_eq!(out, "6\n");
}

#[test]
fn duplicate_variants_and_fields_are_reported() {
    assert!(exec("enum E\n  A\n  A\nend").contains("T112"));
    assert!(exec("enum E\n  A(x: int, x: int)\nend").contains("T126"));
}
