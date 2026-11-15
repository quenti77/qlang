//! Big numbers and dates are in the prelude, but only loaded when a program seems to need
//! them. Whatever way they are reached, a program must behave as if they were always there.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run(main: &str, files: &[(&str, &str)]) -> String {
    let mut host = MemoryHost::default();
    for (name, text) in files {
        host.files.insert((*name).into(), (*text).into());
    }
    let prog = match compile("main.q", main, &mut host) {
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
}

#[test]
fn a_program_that_names_them_has_them() {
    assert_eq!(
        run("print(BigInt.pow(2, 70))", &[]),
        "1180591620717411303424\n"
    );
    assert_eq!(
        run("print(Date.of(2026, 11, 13))", &[]),
        "Date { year: 2026, month: 11, day: 13 }\n"
    );
}

#[test]
fn an_imported_module_may_use_them_alone() {
    let lib = "fun big() -> string\n  \"{BigInt.pow(10, 25)}\"\nend\nfun day() -> string\n  \"{Date.of(2027, 1, 31)}\"\nend\nexport big, day\n";
    let main = "import big, day from \"lib.q\"\nprint(big())\nprint(day())";
    assert_eq!(
        run(main, &[("lib.q", lib)]),
        "10000000000000000000000000\nDate { year: 2027, month: 1, day: 31 }\n"
    );
}

#[test]
fn a_program_without_them_still_reports_its_own_errors() {
    let out = run("print(unknown_name)", &[]);
    assert!(out.starts_with("COMPILE:"), "{out}");
    assert!(!out.contains("Date") && !out.contains("BigInt"), "{out}");
}

#[test]
fn a_local_name_shadows_the_prelude_whether_loaded_or_not() {
    let src = "struct Date\n  public day: int\nend\nprint(Date { day: 3 }.day)";
    assert_eq!(run(src, &[]), "3\n");
}
