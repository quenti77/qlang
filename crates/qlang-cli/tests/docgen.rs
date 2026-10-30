//! `qlang doc`: Markdown from the documentation comments.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-doc-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
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

const SOURCE: &str = "--\" A point in the plane. --\"\nstruct Point\n  --\" Horizontal. --\"\n  public x: int\n  secret: int\nend\n\nimpl Point\n  --\" Squared distance to the origin. --\"\n  public fun norm2(self) -> int\n    self.x * self.x\n  end\n\n  fun hidden(self) -> int\n    1\n  end\nend\n\n--\" Adds two numbers. --\"\nfun add(a: int, b: int) -> int\n  a + b\nend\n\nfun internal() -> int\n  1\nend\n\nenum Color\n  --\" The warm one. --\"\n  Red\n  Blue\nend\n\nexport add, Point, Color\n";

#[test]
fn documents_the_exported_names_and_public_members() {
    let d = Dir::new("basic");
    d.write("shapes.q", SOURCE);
    let (out, err, code) = d.qlang(&["doc", "shapes.q"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.starts_with("# shapes.q\n"), "{out}");
    assert!(
        out.contains("## `struct Point`") && out.contains("A point in the plane."),
        "{out}"
    );
    assert!(out.contains("- `x: int` — Horizontal."), "{out}");
    assert!(
        out.contains("- `fun norm2() -> int` — Squared distance to the origin."),
        "{out}"
    );
    assert!(
        out.contains("## `fun add(a: int, b: int) -> int`") && out.contains("Adds two numbers."),
        "{out}"
    );
    assert!(
        out.contains("- `Color.Red` — The warm one.") && out.contains("- `Color.Blue`"),
        "{out}"
    );
    // not exported, private
    assert!(
        !out.contains("internal") && !out.contains("secret") && !out.contains("hidden"),
        "{out}"
    );
}

#[test]
fn a_file_without_export_documents_everything_top_level() {
    let d = Dir::new("noexport");
    d.write("lib.q", "--\" One. --\"\nfun one() -> int\n  1\nend\n\n--\" Two. --\"\nfun two() -> int\n  2\nend\n");
    let (out, _, _) = d.qlang(&["doc", "lib.q"]);
    assert!(
        out.contains("`fun one() -> int`") && out.contains("`fun two() -> int`"),
        "{out}"
    );
}

#[test]
fn a_project_is_documented_without_tests_and_written_to_a_file() {
    let d = Dir::new("project");
    d.write("qlang.toml", "[package]\nname = \"p\"\n");
    d.write("src/a.q", "--\" Doc of a. --\"\nfun a() -> int\n  1\nend\n");
    d.write("a_test.q", "fun test_a()\n  assert(true)\nend\n");
    d.write(".qlang/packages/x/b.q", "fun hidden() -> int\n  1\nend\n");
    let (out, _, code) = d.qlang(&["doc", "-o", "API.md"]);
    assert_eq!(code, Some(0));
    assert!(out.is_empty());
    let text = fs::read_to_string(d.0.join("API.md")).unwrap();
    assert!(
        text.contains("# src/a.q") && text.contains("Doc of a."),
        "{text}"
    );
    assert!(
        !text.contains("test_a") && !text.contains("hidden"),
        "{text}"
    );
}

#[test]
fn a_file_that_does_not_compile_is_reported() {
    let d = Dir::new("broken");
    d.write("bad.q", "let n: int = \"no\"\n");
    let (_, err, code) = d.qlang(&["doc", "bad.q"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("does not compile"), "{err}");
}

#[test]
fn param_and_return_tags_are_laid_out() {
    let d = Dir::new("tags");
    d.write(
        "lib.q",
        "--\" Adds two numbers.\n  @param a the first one\n  @param b the second one,\n    maybe negative\n  @return their sum --\"\nfun add(a: int, b: int) -> int\n  a + b\nend\n\nstruct Box\n  public n: int\nend\n\nimpl Box\n  --\" Grows the box.\n  @param by how much\n  @returns the new size --\"\n  public fun grow(self, by: int) -> int\n    self.n + by\n  end\nend\n",
    );
    let (out, err, code) = d.qlang(&["doc", "lib.q"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("Adds two numbers.\n\n"), "{out}");
    assert!(!out.contains("@param") && !out.contains("@return"), "{out}");
    assert!(out.contains("- parameter `a` — the first one\n"), "{out}");
    assert!(
        out.contains("- parameter `b` — the second one, maybe negative\n"),
        "{out}"
    );
    assert!(out.contains("- returns their sum\n"), "{out}");
    assert!(
        out.contains("- `fun grow(by: int) -> int` — Grows the box.\n  - parameter `by` — how much\n  - returns the new size\n"),
        "{out}"
    );
}
