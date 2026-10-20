//! Imports through the real `qlang` program, on real directories.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn qlang() -> Command {
    Command::new(env!("CARGO_BIN_EXE_qlang"))
}

/// A fresh directory for one test, removed at the end.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-imports-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).to_string()
}

fn run_in(cwd: &Path, args: &[&str]) -> Output {
    qlang().current_dir(cwd).args(args).output().unwrap()
}

fn project(d: &Dir) {
    d.write(
        "app/main.q",
        "import twice from \"../lib/util.q\"\nprint(twice(21))\n",
    );
    d.write(
        "lib/util.q",
        "fun twice(n: int) -> int\n  n * 2\nend\nexport twice\n",
    );
}

#[test]
fn an_import_can_leave_the_directory_of_the_main_file() {
    let d = Dir::new("up");
    project(&d);
    // from the project root, and from the folder of the main file
    let o = run_in(&d.0, &["run", "app/main.q"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "42\n");
    let o = run_in(&d.0.join("app"), &["run", "main.q"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "42\n");
    let o = run_in(&d.0, &["check", "app/main.q"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
}

#[test]
fn root_confines_imports() {
    let d = Dir::new("root");
    project(&d);
    let o = run_in(&d.0, &["run", "app/main.q", "--root", "app"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("outside of --root"),
        "{}",
        text(&o.stderr)
    );
    // a root that holds everything is fine
    let o = run_in(&d.0, &["run", "app/main.q", "--root", "."]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    // the main file itself must be under the root
    let o = run_in(&d.0, &["run", "lib/util.q", "--root", "app"]);
    assert_eq!(o.status.code(), Some(64));
    // the root must exist
    let o = run_in(&d.0, &["run", "app/main.q", "--root", "nowhere"]);
    assert_eq!(o.status.code(), Some(64));
}

#[test]
fn only_qlang_files_can_be_imported() {
    let d = Dir::new("kind");
    d.write("secret.txt", "top secret line\n");
    d.write("main.q", "import x from \"secret.txt\"\n");
    let o = run_in(&d.0, &["run", "main.q"]);
    assert!(!o.status.success());
    let err = text(&o.stderr);
    assert!(err.contains("only .q files can be imported"), "{err}");
    assert!(
        !err.contains("top secret"),
        "the file content must not leak: {err}"
    );
}

#[test]
fn absolute_paths_are_refused() {
    let d = Dir::new("abs");
    d.write("lib.q", "fun f()\nend\nexport f\n");
    d.write(
        "main.q",
        &format!("import f from \"{}\"\n", d.0.join("lib.q").display()),
    );
    let o = run_in(&d.0, &["run", "main.q"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("not an absolute one"),
        "{}",
        text(&o.stderr)
    );
}

#[cfg(unix)]
#[test]
fn the_same_file_through_a_symbolic_link_is_one_module() {
    let d = Dir::new("link");
    d.write(
        "lib/a.q",
        "print(\"loading a\")\nfun a() -> int\n  1\nend\nexport a\n",
    );
    std::os::unix::fs::symlink(d.0.join("lib/a.q"), d.0.join("alias.q")).unwrap();
    d.write(
        "main.q",
        "import a from \"lib/a.q\"\nimport \"alias.q\" as again\nprint(a() + again.a())\n",
    );
    let o = run_in(&d.0, &["run", "main.q"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "loading a\n2\n");
}

#[cfg(unix)]
#[test]
fn a_cyclic_import_through_a_link_is_matched_to_the_same_module() {
    // `b.q` reaches `a.q` through a symlink; since neither file runs top-level code,
    // the cycle has a valid order, but only if the link is recognised as the same
    // module `a.q` imports from directly (otherwise `a()` below would not resolve).
    let d = Dir::new("cycle");
    d.write(
        "a.q",
        "import b from \"b.q\"\nfun a() -> int\n  1\nend\nexport a\nprint(b())\n",
    );
    d.write(
        "b.q",
        "import a from \"link.q\"\nfun b() -> int\n  a() + 1\nend\nexport b\n",
    );
    std::os::unix::fs::symlink(d.0.join("a.q"), d.0.join("link.q")).unwrap();
    let o = run_in(&d.0, &["run", "a.q"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), "2\n");
}

#[cfg(unix)]
#[test]
fn a_cycle_with_top_level_code_in_more_than_one_module_has_no_valid_order() {
    let d = Dir::new("cycle_runtime");
    d.write(
        "a.q",
        "import b from \"b.q\"\nfun a()\nend\nexport a\nprint(\"a\")\n",
    );
    d.write(
        "b.q",
        "import a from \"a.q\"\nfun b()\nend\nexport b\nprint(\"b\")\n",
    );
    let o = run_in(&d.0, &["run", "a.q"]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("cycle"), "{}", text(&o.stderr));
}

fn json(args: &[&str], request: &str) -> serde_json::Value {
    let mut child = qlang()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.as_bytes())
        .unwrap();
    let o = child.wait_with_output().unwrap();
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn json_requests_use_the_same_import_rules_without_touching_the_disk() {
    // the editor sends paths relative to the project root; the terminal resolves them
    // relative to each file: both reach `common.q`
    let req = serde_json::json!({
        "entry": "sub/main.q",
        "files": {
            "sub/main.q": "import helper from \"../common.q\"\nprint(helper())",
            "common.q": "fun helper() -> int\n  7\nend\nexport helper"
        },
        "input": []
    })
    .to_string();
    let r = json(&["check", "--json"], &req);
    assert_eq!(r["ok"], true, "{r}");
    let r = json(&["run", "--json"], &req);
    assert_eq!(r["output"], "7\n");
    // a file outside `files` does not exist, whatever the disk holds
    let req = serde_json::json!({
        "files": { "main.q": "import x from \"../../../../etc/hostname.q\"" }
    })
    .to_string();
    let r = json(&["run", "--json"], &req);
    assert_eq!(r["ok"], false);
    assert!(
        r["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "{r}"
    );
}
