//! `import ... from "@name/file.q"`: packages declared in `qlang.toml`, through the real
//! `qlang` program on real directories.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-packages-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
    }

    fn qlang(&self, cwd: &str, args: &[&str]) -> (String, String, bool) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(self.0.join(cwd))
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
            o.status.success(),
        )
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `app` depends on `mail`, which depends on `text`; all three side by side.
fn workspace(name: &str) -> Dir {
    let d = Dir::new(name);
    d.write(
        "app/qlang.toml",
        "[dependencies]\nmail = { path = \"../mail\" }\n",
    );
    d.write("mail/qlang.toml", "[package]\nname = \"mail\"\nmain = \"src/mail.q\"\n\n[dependencies]\ntext = { path = \"../text\" }\n");
    d.write(
        "mail/src/mail.q",
        "import shout from \"@text/shout.q\"\nimport helper from \"./helper.q\"\nfun greet(who: string) -> string\n  shout(helper(who))\nend\nexport greet\n",
    );
    d.write(
        "mail/src/helper.q",
        "fun helper(w: string) -> string\n  \"hello \" + w\nend\nexport helper\n",
    );
    d.write(
        "mail/send.q",
        "fun send() -> string\n  \"sent\"\nend\nexport send\n",
    );
    d.write("text/qlang.toml", "[package]\nname = \"text\"\n");
    d.write(
        "text/shout.q",
        "fun shout(s: string) -> string\n  s.upper() + \"!\"\nend\nexport shout\n",
    );
    d
}

#[test]
fn a_path_dependency_can_be_imported_by_name() {
    let d = workspace("basic");
    d.write(
        "app/main.q",
        "import \"@mail\" as m\nimport send from \"@mail/send.q\"\nprint(m.greet(\"bob\"))\nprint(send())\n",
    );
    let (out, err, ok) = d.qlang("app", &["run", "main.q"]);
    assert!(ok, "{err}");
    assert_eq!(out, "HELLO BOB!\nsent\n");
}

#[test]
fn a_package_that_is_not_a_dependency_is_refused() {
    let d = workspace("missing");
    d.write("app/main.q", "import f from \"@nothing/x.q\"\nprint(1)\n");
    let (_, err, ok) = d.qlang("app", &["run", "main.q"]);
    assert!(!ok);
    assert!(
        err.contains("package `nothing` is not a dependency"),
        "{err}"
    );
}

#[test]
fn a_package_cannot_import_out_of_itself() {
    let d = workspace("escape");
    d.write(
        "mail/src/evil.q",
        "import x from \"../../app/main.q\"\nexport x\n",
    );
    d.write("app/main.q", "import \"@mail/src/evil.q\" as e\nprint(1)\n");
    let (_, err, ok) = d.qlang("app", &["run", "main.q"]);
    assert!(!ok);
    assert!(
        err.contains("T100") || err.contains("cannot import"),
        "{err}"
    );
}

#[test]
fn root_does_not_confine_declared_packages() {
    let d = workspace("root");
    d.write(
        "app/main.q",
        "import \"@mail\" as m\nprint(m.greet(\"a\"))\n",
    );
    let (out, err, ok) = d.qlang("app", &["run", "main.q", "--root", "."]);
    assert!(ok, "{err}");
    assert_eq!(out, "HELLO A!\n");
}

#[test]
fn errors_name_the_package_file() {
    let d = workspace("diag");
    d.write("mail/bad.q", "let x: int = \"no\"\n");
    d.write("app/main.q", "import \"@mail/bad.q\" as b\n");
    let (_, err, ok) = d.qlang("app", &["check", "main.q"]);
    assert!(!ok);
    assert!(err.contains("@mail/bad.q"), "{err}");
}

#[test]
fn two_directories_for_one_name_are_a_conflict() {
    let d = workspace("conflict");
    d.write("other/text/qlang.toml", "[package]\nname = \"text\"\n");
    d.write(
        "mail/qlang.toml",
        "[dependencies]\ntext = { path = \"../other/text\" }\n",
    );
    d.write(
        "app/qlang.toml",
        "[dependencies]\ntext = { path = \"../text\" }\nmail = { path = \"../mail\" }\n",
    );
    d.write("app/main.q", "print(1)\n");
    let (_, err, ok) = d.qlang("app", &["run", "main.q"]);
    assert!(!ok);
    assert!(err.contains("one version of each package"), "{err}");
}

#[test]
fn a_git_dependency_must_be_installed_first() {
    let d = Dir::new("git");
    d.write(
        "app/qlang.toml",
        "[dependencies]\nx = { git = \"https://example.com/x\" }\n",
    );
    d.write("app/main.q", "print(1)\n");
    let (_, err, ok) = d.qlang("app", &["run", "main.q"]);
    assert!(!ok);
    assert!(
        err.contains("package `x` is not installed (run `qlang install`)"),
        "{err}"
    );
}
