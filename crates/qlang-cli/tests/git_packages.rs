//! `qlang install | update | add | remove` with git packages, on local repositories.
//! Skipped when `git` is not installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-gitpkg-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, text: &str) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap_or_default()
    }

    /// A repository `name` with a single file `lib.q` printing `body`, tagged `tag`.
    fn repo(&self, name: &str, files: &[(&str, &str)], tag: &str) -> String {
        let dir = self.0.join("repos").join(name);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]);
        for (f, t) in files {
            fs::write(dir.join(f), t).unwrap();
        }
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "--quiet", "-m", "first"]);
        git(&dir, &["tag", tag]);
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A new commit (and optionally tag) on a repository made by [`Dir::repo`].
    fn advance(&self, name: &str, file: &str, text: &str, tag: Option<&str>) {
        let dir = self.0.join("repos").join(name);
        fs::write(dir.join(file), text).unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "--quiet", "-m", "next"]);
        if let Some(t) = tag {
            git(&dir, &["tag", t]);
        }
    }

    fn qlang(&self, args: &[&str]) -> (String, String, bool) {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(args)
            .current_dir(self.0.join("app"))
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

const APP: &str = "import \"@mail\" as m\nprint(m.version())\n";

fn lib(version: &str) -> String {
    format!("fun version() -> string\n  \"{version}\"\nend\nexport version\n")
}

#[test]
fn install_fetches_a_tag_and_writes_the_lock() {
    if !have_git() {
        return;
    }
    let d = Dir::new("install");
    let url = d.repo("mail", &[("lib.q", &lib("v1"))], "v1.0.0");
    d.write(
        "app/qlang.toml",
        &format!("[dependencies]\nmail = {{ git = \"{url}\", tag = \"v1.0.0\" }}\n"),
    );
    d.write("app/main.q", APP);

    let (_, err, ok) = d.qlang(&["run", "main.q"]);
    assert!(!ok);
    assert!(err.contains("not installed (run `qlang install`)"), "{err}");

    let (out, err, ok) = d.qlang(&["install"]);
    assert!(ok, "{err}");
    assert!(out.contains("mail tag:v1.0.0 ->"), "{out}");
    let (out, err, ok) = d.qlang(&["run", "main.q"]);
    assert!(ok, "{err}");
    assert_eq!(out, "v1\n");
    let lock = d.read("app/qlang.lock");
    assert!(
        lock.contains("name = \"mail\"") && lock.contains("ref = \"tag:v1.0.0\""),
        "{lock}"
    );
}

#[test]
fn the_lock_pins_a_branch_until_update() {
    if !have_git() {
        return;
    }
    let d = Dir::new("lock");
    let url = d.repo("mail", &[("lib.q", &lib("one"))], "v1");
    d.write(
        "app/qlang.toml",
        &format!("[dependencies]\nmail = {{ git = \"{url}\", branch = \"main\" }}\n"),
    );
    d.write("app/main.q", APP);
    assert!(d.qlang(&["install"]).2);
    assert_eq!(d.qlang(&["run", "main.q"]).0, "one\n");

    d.advance("mail", "lib.q", &lib("two"), None);
    // installing again follows the lock
    assert!(d.qlang(&["install"]).2);
    assert_eq!(d.qlang(&["run", "main.q"]).0, "one\n");
    // updating moves to the new commit
    assert!(d.qlang(&["update"]).2);
    assert_eq!(d.qlang(&["run", "main.q"]).0, "two\n");
    // and a fresh checkout (no .qlang) installs the locked commit
    fs::remove_dir_all(d.0.join("app/.qlang")).unwrap();
    assert!(d.qlang(&["install"]).2);
    assert_eq!(d.qlang(&["run", "main.q"]).0, "two\n");
}

#[test]
fn add_and_remove_edit_qlang_toml() {
    if !have_git() {
        return;
    }
    let d = Dir::new("add");
    let url = d.repo("mail", &[("lib.q", &lib("v1"))], "v1.0.0");
    d.write(
        "app/qlang.toml",
        "# my project\n[package]\nname = \"app\"\n",
    );
    d.write("app/main.q", APP);
    let (_, err, ok) = d.qlang(&["add", "mail", "--git", &url, "--tag", "v1.0.0"]);
    assert!(ok, "{err}");
    let toml = d.read("app/qlang.toml");
    assert!(
        toml.contains("# my project") && toml.contains("mail ="),
        "{toml}"
    );
    assert_eq!(d.qlang(&["run", "main.q"]).0, "v1\n");

    let (_, err, ok) = d.qlang(&["remove", "mail"]);
    assert!(ok, "{err}");
    assert!(!d.read("app/qlang.toml").contains("mail"));
    assert!(!d.0.join("app/.qlang/packages/mail").exists());
    assert_eq!(d.read("app/qlang.lock"), "");
}

#[test]
fn packages_of_packages_are_installed_flat() {
    if !have_git() {
        return;
    }
    let d = Dir::new("flat");
    let text = d.repo(
        "text",
        &[(
            "lib.q",
            "fun shout(s: string) -> string\n  s.upper()\nend\nexport shout\n",
        )],
        "v1",
    );
    let mail = d.repo(
        "mail",
        &[
            (
                "qlang.toml",
                &format!("[dependencies]\ntext = {{ git = \"{text}\", tag = \"v1\" }}\n"),
            ),
            (
                "lib.q",
                "import shout from \"@text\"\nfun version() -> string\n  shout(\"deep\")\nend\nexport version\n",
            ),
        ],
        "v1",
    );
    d.write(
        "app/qlang.toml",
        &format!("[dependencies]\nmail = {{ git = \"{mail}\", tag = \"v1\" }}\n"),
    );
    d.write("app/main.q", APP);
    let (_, err, ok) = d.qlang(&["install"]);
    assert!(ok, "{err}");
    assert!(d.0.join("app/.qlang/packages/text").is_dir());
    assert_eq!(d.qlang(&["run", "main.q"]).0, "DEEP\n");
}

#[test]
fn a_missing_tag_is_reported() {
    if !have_git() {
        return;
    }
    let d = Dir::new("notag");
    let url = d.repo("mail", &[("lib.q", &lib("v1"))], "v1");
    d.write(
        "app/qlang.toml",
        &format!("[dependencies]\nmail = {{ git = \"{url}\", tag = \"nope\" }}\n"),
    );
    let (_, err, ok) = d.qlang(&["install"]);
    assert!(!ok);
    assert!(err.contains("has no tag:nope"), "{err}");
}

#[test]
fn only_safe_transports_are_allowed() {
    if !have_git() {
        return;
    }
    let d = Dir::new("ext");
    d.write(
        "app/qlang.toml",
        "[dependencies]\nx = { git = \"ext::sh -c 'touch /tmp/qlang-pwned'\" }\n",
    );
    let (_, _, ok) = d.qlang(&["install"]);
    assert!(!ok);
    assert!(!Path::new("/tmp/qlang-pwned").exists());
}

#[test]
fn install_puts_back_files_edited_by_hand() {
    if !have_git() {
        return;
    }
    let d = Dir::new("tamper");
    let url = d.repo("mail", &[("lib.q", &lib("v1"))], "v1");
    d.write(
        "app/qlang.toml",
        &format!("[dependencies]\nmail = {{ git = \"{url}\", tag = \"v1\" }}\n"),
    );
    d.write("app/main.q", APP);
    assert!(d.qlang(&["install"]).2);
    d.write("app/.qlang/packages/mail/lib.q", &lib("hacked"));
    assert_eq!(d.qlang(&["run", "main.q"]).0, "hacked\n");
    assert!(d.qlang(&["install"]).2);
    assert_eq!(d.qlang(&["run", "main.q"]).0, "v1\n");
}
