//! `Term` is the terminal of the `qlang` command: a host without one reports its use before
//! running; the `\e` escape writes the ESC character.

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

#[test]
fn every_use_of_term_is_reported_when_the_host_has_no_terminal() {
    for call in [
        "Term.size()",
        "Term.is_tty()",
        "Term.colors()",
        "Term.raw(true)",
        "Term.write(\"x\")",
        "Term.frame(\"x\")",
        "Term.clear()",
        "Term.mouse(true)",
        "Term.alt_screen(true)",
        "Term.event(0)",
        "Term.session(fun() -> bool true end)",
    ] {
        let out = exec(&format!("let x = {call}"));
        assert!(
            out.starts_with("COMPILE: T250") && out.contains("only offered by the `qlang` command"),
            "{call}: {out}"
        );
    }
}

#[test]
fn the_escape_e_is_the_esc_character() {
    let out = exec("print(\"\\e[0m\".len())\nprint(\"\\e\".to_bytes().hex())");
    assert_eq!(out.replace('\n', " ").trim(), "4 1b");
}

#[test]
fn an_unknown_escape_still_lists_the_allowed_ones() {
    let out = exec("let s = \"\\q\"");
    assert!(out.contains("P020") && out.contains("\\e"), "{out}");
}
