//! Documentation comments (`--" ... --"`): attached to the declaration right below them and
//! reported to editors.

use qlang_core::check::{CompileOptions, Info, compile_with};
use qlang_core::host::MemoryHost;

fn info(files: &[(&str, &str)]) -> Info {
    let mut host = MemoryHost::default();
    for (name, text) in files {
        host.files.insert(name.to_string(), text.to_string());
    }
    let main = files[0].1;
    match compile_with(files[0].0, main, &mut host, &CompileOptions { info: true }) {
        Ok(p) => p.info.unwrap(),
        Err(e) => panic!("{}", e.render()),
    }
}

fn item<'a>(info: &'a Info, name: &str) -> &'a qlang_core::check::ItemInfo {
    // the prelude's items come first and may share a name (`Router.add`): the program's win
    info.items.iter().rev().find(|i| i.name == name).unwrap_or_else(|| panic!("no item {name}"))
}

const SOURCE: &str = r#"
--" A point on the plane. --"
struct Point
  --" Horizontal position. --"
  public x: int
  public y: int
end

impl Point
  --"
    Distance to the origin.

    Always positive.
  --"
  public fun norm(self) -> float
    (self.x * self.x + self.y * self.y).sqrt()
  end
end

--" The colors. --"
enum Color
  --" Warm. --"
  Red
  Blue
end

--" Things that can speak. --"
trait Speaker
  --" Says something. --"
  fun speak(self) -> string
end

--" Adds two numbers. --"
fun add(a: int, b: int) -> int
  a + b
end

--" The answer. --"
const ANSWER = 42

--" Separated by a blank line: documents nothing. --"

fun lonely() -> int
  1
end

let p = Point { x: 1, y: 2 }
print(add(p.x, ANSWER))
let c = Color.Red
print(c)
"#;

#[test]
fn comments_are_attached_to_the_declaration_below() {
    let i = info(&[("main.q", SOURCE)]);
    assert_eq!(item(&i, "Point").doc.as_deref(), Some("A point on the plane."));
    assert_eq!(item(&i, "x").doc.as_deref(), Some("Horizontal position."));
    assert_eq!(item(&i, "y").doc, None);
    assert_eq!(item(&i, "norm").doc.as_deref(), Some("Distance to the origin.\n\nAlways positive."));
    assert_eq!(item(&i, "Color").doc.as_deref(), Some("The colors."));
    assert_eq!(item(&i, "Red").doc.as_deref(), Some("Warm."));
    assert_eq!(item(&i, "Blue").doc, None);
    assert_eq!(item(&i, "Speaker").doc.as_deref(), Some("Things that can speak."));
    assert_eq!(item(&i, "add").doc.as_deref(), Some("Adds two numbers."));
    assert_eq!(item(&i, "ANSWER").doc.as_deref(), Some("The answer."));
}

#[test]
fn a_blank_line_detaches_the_comment() {
    let i = info(&[("main.q", SOURCE)]);
    assert_eq!(item(&i, "lonely").doc, None);
}

#[test]
fn members_and_uses_carry_the_documentation() {
    let i = info(&[("main.q", SOURCE)]);
    let members = &i.members["Point"];
    assert_eq!(members.iter().find(|m| m.name == "x").unwrap().doc.as_deref(), Some("Horizontal position."));
    assert!(members.iter().find(|m| m.name == "norm").unwrap().doc.as_deref().unwrap().starts_with("Distance"));
    // a call to `add`, and the name of a type used as a value, say what they stand for
    let call = i.exprs.iter().find(|e| e.label.as_deref().is_some_and(|l| l.starts_with("fun add"))).expect("the callee is reported");
    assert_eq!(call.doc.as_deref(), Some("Adds two numbers."));
    let statics = &i.statics["type Color"];
    assert_eq!(statics.iter().find(|m| m.name == "Red").unwrap().doc.as_deref(), Some("Warm."));
    // the names of the file
    let globals = &i.globals["main.q"];
    assert_eq!(globals.iter().find(|g| g.name == "add").unwrap().doc.as_deref(), Some("Adds two numbers."));
    assert_eq!(globals.iter().find(|g| g.name == "Point").unwrap().doc.as_deref(), Some("A point on the plane."));
}

#[test]
fn built_in_methods_keep_their_own_explanations() {
    let i = info(&[("main.q", "let xs = [1]\nprint(xs.len())")]);
    let len = i.members["array<int>"].iter().find(|m| m.name == "len").unwrap();
    assert_eq!(len.doc.as_deref(), Some("Number of elements."));
}

#[test]
fn documentation_of_imported_modules_is_reported_too() {
    let main = "import add from \"lib.q\"\nprint(add(1, 2))";
    let lib = "--\" Adds. --\"\nfun add(a: int, b: int) -> int\n  a + b\nend\nexport add";
    let i = info(&[("main.q", main), ("lib.q", lib)]);
    assert_eq!(item(&i, "add").doc.as_deref(), Some("Adds."));
    let g = i.globals["main.q"].iter().find(|g| g.name == "add").unwrap();
    assert_eq!(g.doc.as_deref(), Some("Adds."));
}

#[test]
fn a_comment_documents_only_one_declaration() {
    let src = "--\" Only the first. --\"\nfun a() -> int\n  1\nend\nfun b() -> int\n  2\nend\nprint(a() + b())";
    let i = info(&[("main.q", src)]);
    assert_eq!(item(&i, "a").doc.as_deref(), Some("Only the first."));
    assert_eq!(item(&i, "b").doc, None);
}

#[test]
fn doc_comments_do_not_change_the_program() {
    // as every comment, they are ignored when running
    let mut host = MemoryHost::default();
    let p = qlang_core::check::compile("main.q", "--\" doc --\"\nfun f() -> int\n  1\nend\nprint(f())", &mut host).ok().unwrap();
    p.run(&mut host, qlang_core::host::Limits::default());
    assert_eq!(host.output, "1\n");
}
