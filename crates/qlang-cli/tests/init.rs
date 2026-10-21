//! `qlang init` and `qlang run` / `qlang check` without a file (the `entry` of qlang.toml).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-init-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn qlang(&self, cwd: &str, args: &[&str]) -> (String, String, Option<i32>) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(self.0.join(cwd))
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
            o.status.code(),
        )
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap_or_default()
    }

    fn exists(&self, rel: &str) -> bool {
        Path::new(&self.0.join(rel)).exists()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn init_makes_a_project_that_runs() {
    let d = Dir::new("new");
    let (out, err, code) = d.qlang(".", &["init", "My App"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(
        out.contains("created") && out.contains("qlang.toml"),
        "{out}"
    );
    let toml = d.read("My App/qlang.toml");
    assert!(
        toml.contains("name = \"my-app\"") && toml.contains("entry = \"main.q\""),
        "{toml}"
    );
    assert_eq!(d.read("My App/.gitignore"), ".qlang/\n");
    let (out, err, code) = d.qlang("My App", &["run"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "Hello, qlang!\n");
    let (_, err, code) = d.qlang("My App", &["check"]);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn init_never_overwrites_a_project() {
    let d = Dir::new("again");
    fs::create_dir_all(d.0.join("p")).unwrap();
    assert_eq!(d.qlang("p", &["init"]).2, Some(0));
    fs::write(d.0.join("p/main.q"), "print(\"mine\")\n").unwrap();
    let (_, err, code) = d.qlang("p", &["init"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("already a project"), "{err}");
    assert_eq!(d.read("p/main.q"), "print(\"mine\")\n");
}

#[test]
fn init_keeps_the_files_it_finds() {
    let d = Dir::new("keep");
    fs::create_dir_all(d.0.join("p")).unwrap();
    fs::write(d.0.join("p/main.q"), "print(\"mine\")\n").unwrap();
    fs::write(d.0.join("p/.gitignore"), "target\n").unwrap();
    assert_eq!(d.qlang("p", &["init"]).2, Some(0));
    assert_eq!(d.read("p/main.q"), "print(\"mine\")\n");
    assert_eq!(d.read("p/.gitignore"), "target\n");
    assert!(d.exists("p/qlang.toml"));
}

#[test]
fn run_finds_the_entry_from_a_subdirectory() {
    let d = Dir::new("sub");
    fs::create_dir_all(d.0.join("p/src/deep")).unwrap();
    fs::write(
        d.0.join("p/qlang.toml"),
        "[package]\nentry = \"src/app.q\"\n",
    )
    .unwrap();
    fs::write(d.0.join("p/src/app.q"), "print(\"from src\")\n").unwrap();
    let (out, err, code) = d.qlang("p/src/deep", &["run"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "from src\n");
}

#[test]
fn without_an_entry_run_asks_for_a_file() {
    let d = Dir::new("none");
    fs::create_dir_all(d.0.join("p")).unwrap();
    let (_, err, code) = d.qlang("p", &["run"]);
    assert_eq!(code, Some(64));
    assert!(err.contains("missing file"), "{err}");
    fs::write(d.0.join("p/qlang.toml"), "[package]\nentry = \"gone.q\"\n").unwrap();
    let (_, err, code) = d.qlang("p", &["run"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("the entry \"gone.q\" does not exist"), "{err}");
}
