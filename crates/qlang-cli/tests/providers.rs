//! `extern fun` implemented by a provider process (`qlang.toml` and `--extern`), through
//! the real `qlang` program. The provider is a Python script; the tests are skipped when
//! there is no Python on the machine.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PROVIDER: &str = r#"
import sys, json
for line in sys.stdin:
    msg = json.loads(line)
    if msg["op"] == "list":
        out = {"externs": ["shout", "add", "boom"]}
    elif msg["name"] == "shout":
        out = {"ok": msg["args"][0].upper() + "!"}
    elif msg["name"] == "add":
        out = {"ok": msg["args"][0] + msg["args"][1]}
    else:
        out = {"error": "it broke"}
    print(json.dumps(out), flush=True)
"#;

fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-providers-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("provider.py"), PROVIDER).unwrap();
        Dir(p)
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }

    fn run(&self, args: &[&str]) -> (String, String, bool) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(&self.0)
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

const PROGRAM: &str = r#"extern fun shout(s: string) -> string
extern fun add(a: int, b: int) -> int
extern fun boom() -> Result<int, Error>
print(shout("hi"))
print(add(2, 40))
print(boom().unwrap_err().message)
"#;

#[test]
fn a_qlang_toml_declares_the_providers() {
    let Some(py) = python() else { return };
    let d = Dir::new("toml");
    d.write(
        "qlang.toml",
        &format!("[externs]\nhelper = \"{py} provider.py\"\n"),
    );
    d.write("app.q", PROGRAM);
    let (out, err, ok) = d.run(&["run", "app.q"]);
    assert!(ok, "{err}");
    assert_eq!(out, "HI!\n42\nit broke\n");
}

#[test]
fn the_extern_option_declares_a_provider() {
    let Some(py) = python() else { return };
    let d = Dir::new("option");
    d.write("app.q", PROGRAM);
    let cmd = format!("{py} provider.py");
    let (out, err, ok) = d.run(&["run", "app.q", "--extern", &cmd]);
    assert!(ok, "{err}");
    assert_eq!(out, "HI!\n42\nit broke\n");
}

#[test]
fn without_a_provider_using_an_extern_is_a_compile_error() {
    let d = Dir::new("none");
    d.write("app.q", PROGRAM);
    let (_, err, ok) = d.run(&["run", "app.q"]);
    assert!(!ok);
    assert!(err.contains("T263"), "{err}");
}

#[test]
fn a_provider_that_cannot_start_is_reported() {
    let d = Dir::new("missing");
    d.write("qlang.toml", "[externs]\nx = \"no-such-program-here\"\n");
    d.write("app.q", "print(1)");
    let (_, err, ok) = d.run(&["run", "app.q"]);
    assert!(!ok);
    assert!(err.contains("provider `x`: cannot start"), "{err}");
}

#[test]
fn the_toml_is_found_in_a_parent_directory() {
    let Some(py) = python() else { return };
    let d = Dir::new("parent");
    d.write(
        "qlang.toml",
        &format!("[externs]\nhelper = \"{py} provider.py\"\n"),
    );
    fs::create_dir_all(d.0.join("src")).unwrap();
    d.write("src/app.q", PROGRAM);
    let (out, err, ok) = d.run(&["run", "src/app.q"]);
    assert!(ok, "{err}");
    assert!(out.starts_with("HI!"), "{out}");
}

fn json_request(mode: &str, d: &Dir) -> String {
    use std::io::Write;
    let request = serde_json::json!({
        "entry": "app.q",
        "files": { "app.q": "extern fun shout(s: string) -> string\nprint(shout(\"hi\"))" },
        "mode": mode,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "--json"])
        .current_dir(&d.0)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request.to_string().as_bytes())
        .unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn a_json_check_assumes_externs_are_provided_but_a_json_run_does_not() {
    let d = Dir::new("json");
    let checked = json_request("check", &d);
    assert!(checked.contains("\"ok\":true"), "{checked}");
    let run = json_request("run", &d);
    assert!(run.contains("T263"), "{run}");
}
