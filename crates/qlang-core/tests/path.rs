//! `Path`: file paths as text, `/` between the parts. Nothing touches the disk.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {} {}", d.code, d.message),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => e.render(),
    }
}

#[test]
fn normalize_joins_and_splits() {
    let src = r#"
print(Path.normalize("a//b/./c/../d/"))
print(Path.normalize("/../a"))
print(Path.normalize("../x/../.."))
print(Path.normalize(""))
print(Path.normalize("C:/a/../b"))
print(Path.join("/srv", "app/uploads"))
print(Path.join("a", "/abs"))
print(Path.join_all(["a", "", "b/", "../c"]))
print(Path.join_all([]) == "")
print("{Path.base("/a/b.txt")} {Path.base("/a/b/")} {Path.base("/")} {Path.base("x")} {Path.base("")}")
print("{Path.dir("/a/b.txt")} {Path.dir("b.txt")} {Path.dir("/a")} {Path.dir("a/b/")}")
"#;
    assert_eq!(
        run(src),
        "a/b/d\n/a\n../..\n.\nC:/b\n/srv/app/uploads\n/abs\na/c\ntrue\nb.txt b / x .\n/a . / a\n"
    );
}

#[test]
fn extensions() {
    let src = r#"
print("{Path.ext("a.tar.gz")}|{Path.ext(".bashrc")}|{Path.ext("a.")}|{Path.ext("dir.d/file")}|{Path.ext("photo.PNG")}")
print("{Path.stem("a.tar.gz")} {Path.stem(".bashrc")} {Path.stem("/x/y.md")}")
print("{Path.with_ext("a/b.txt", "md")} {Path.with_ext("a/b", "md")} {Path.with_ext("a.b.c", "")}")
print(Path.is_absolute("/a"))
print(Path.is_absolute("a/b"))
print(Path.is_absolute("C:/x"))
"#;
    assert_eq!(
        run(src),
        "gz||||PNG\na.tar .bashrc y\na/b.md a/b.md a.b\ntrue\nfalse\ntrue\n"
    );
}

#[test]
fn glob_patterns() {
    let src = r#"
fun check(pattern: string, text: string, expected: bool)
  if Path.matches(pattern, text) != expected then
    print("WRONG {pattern} {text}")
  end
end
check("*.q", "a.q", true)
check("*.q", "d/a.q", false)
check("**/*.q", "d/e/a.q", true)
check("**/*.q", "a.q", true)
check("src/**", "src/a/b", true)
check("a?c", "abc", true)
check("a?c", "a/c", false)
check("[a-c]x", "bx", true)
check("[!a-c]x", "dx", true)
check("[!a-c]x", "bx", false)
check("a.b", "axb", false)
check("a(b)+", "a(b)+", true)
check("\{x\}", "\{x\}", true)
check("a[", "a[", true)
check("", "", true)
print("done")
"#;
    assert_eq!(run(src), "done\n");
}
