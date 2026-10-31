//! `#[Name(args)]`: annotations are declared by `#[annotation]` structs and checked at compile time.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

/// The codes of the errors (sorted, comma separated), or the output when it compiles.
fn run(src: &str) -> String {
    let mut host = MemoryHost::default();
    match compile("main.q", src, &mut host) {
        Ok(p) => {
            let r = p.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => host.output,
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            codes.join(",")
        }
    }
}

const DECLS: &str = "#[annotation]\nstruct Route\n  path: string\n  method: string = \"GET\"\nend\n\n#[annotation]\nstruct Min\n  value: int\nend\n\n#[annotation]\nstruct Tags\n  names: array<string>\nend\n\n";

fn with(body: &str) -> String {
    format!("{DECLS}{body}")
}

#[test]
fn annotations_go_on_every_kind_of_declaration() {
    let src = with(
        "-- documented
#[Route(\"/users\")]
#[Tags([\"a\", \"b\"])]
fun users() -> int
  1
end

#[Route(\"/x\", method: \"POST\")]
struct Form
  #[Min(0)]
  age: int
  #[Min(-5), Tags([\"id\"])]
  id: int
end

#[Tags([])]
enum Color
  #[Route(\"/red\")]
  Red
  Green
end

#[Tags([\"t\"])]
trait Named
  #[Route(\"/name\")]
  fun name(self) -> string
end

impl Named for Form
  #[Route(\"/m\")]
  fun name(self) -> string
    \"form\"
  end
end

fun check(#[Min(1)] n: int) -> int
  n
end

print(users() + check(2))
",
    );
    assert_eq!(run(&src), "3\n");
}

#[test]
fn a_doc_comment_above_an_annotation_still_documents() {
    let src =
        with("-- the users\n#[Route(\"/u\")]\nfun users() -> int\n  1\nend\nprint(users())\n");
    assert_eq!(run(&src), "1\n");
}

#[test]
fn mistakes_are_reported() {
    // `#[used]` is built in and takes nothing
    assert_eq!(run("#[used]\nfun f()\nend\n"), "");
    assert_eq!(run("#[used(1)]\nfun f()\nend\n"), "T292");
    // unknown annotation
    assert_eq!(run("#[Nope]\nfun f()\nend\n"), "T001");
    // a struct that is not an annotation
    assert_eq!(
        run("struct S\n  a: int\nend\n#[S(1)]\nfun f()\nend\n"),
        "T290"
    );
    // a missing argument, an unknown field, too many, twice
    assert_eq!(run(&with("#[Route]\nfun f()\nend\n")), "T293");
    assert_eq!(
        run(&with("#[Route(\"/\", verb: \"GET\")]\nfun f()\nend\n")),
        "T292"
    );
    assert_eq!(
        run(&with("#[Route(\"/\", \"GET\", \"x\")]\nfun f()\nend\n")),
        "T292"
    );
    assert_eq!(
        run(&with("#[Route(\"/\", path: \"/\")]\nfun f()\nend\n")),
        "T292"
    );
    // a wrong type
    assert_eq!(run(&with("#[Min(\"zero\")]\nfun f()\nend\n")), "T002");
    // not a literal
    assert_eq!(run(&with("const N = 3\n#[Min(N)]\nfun f()\nend\n")), "T294");
    assert_eq!(run(&with("#[Min(1 + 2)]\nfun f()\nend\n")), "T294");
    // annotations on a statement
    assert_eq!(run(&with("#[Min(1)]\nlet x = 1\n")), "P051");
    // `#[annotation]` on something else, with arguments
    assert_eq!(run("#[annotation]\nfun f()\nend\n"), "T291");
    assert_eq!(run("#[annotation(1)]\nstruct A\nend\n"), "T291");
    // an annotation struct is plain data
    assert_eq!(run("#[annotation]\nstruct A<T>\n  a: T\nend\n"), "T295");
}
