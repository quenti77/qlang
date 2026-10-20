//! Package paths (`@name/file.q`) are plain module names for the host.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
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

fn host() -> MemoryHost {
    let mut host = MemoryHost::default();
    host.files.insert(
        "@util/lib.q".into(),
        "import double from \"./two.q\"\nfun twice(x: int) -> int\n  double(x)\nend\nexport twice"
            .into(),
    );
    host.files.insert(
        "@util/two.q".into(),
        "fun double(x: int) -> int\n  x * 2\nend\nexport double".into(),
    );
    host
}

#[test]
fn a_package_file_and_its_relative_imports_load_from_the_host() {
    let out = exec(
        "import twice from \"@util/lib.q\"\nprint(twice(21))",
        host(),
    );
    assert_eq!(out, "42\n");
}

#[test]
fn a_package_cannot_reach_outside_itself() {
    let mut h = host();
    h.files
        .insert("@util/evil.q".into(), "import x from \"../main.q\"".into());
    let out = exec("import \"@util/evil.q\" as e", h);
    assert!(out.starts_with("COMPILE: T100"), "{out}");
}
