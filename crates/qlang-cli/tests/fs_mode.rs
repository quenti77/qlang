//! The permission bits of a file: `fs.write(.., mode:)`, `fs.chmod` and `fs.mode` (unix).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-fsmode-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn mode(&self, file: &str) -> u32 {
        fs::metadata(self.0.join(file))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(d: &Dir, program: &str) -> (String, String, Option<i32>) {
    fs::write(d.0.join("a.q"), program).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "a.q"])
        .current_dir(&d.0)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
        o.status.code(),
    )
}

#[test]
fn a_file_is_created_with_the_mode_it_is_given() {
    let d = Dir::new("write");
    let (out, err, code) = run(
        &d,
        "fs.write(\"secret.env\", \"k=v\", mode: 0o600).unwrap()\nfs.write_bytes(\"blob\", \"x\".to_bytes(), mode: 0o640).unwrap()\nprint(fs.read(\"secret.env\").unwrap())\n",
    );
    assert_eq!((out.as_str(), code), ("k=v\n", Some(0)), "{err}");
    assert_eq!(d.mode("secret.env"), 0o600);
    assert_eq!(d.mode("blob"), 0o640);
}

#[test]
fn a_file_that_was_there_gets_the_mode_too_and_without_a_mode_nothing_changes() {
    let d = Dir::new("again");
    fs::write(d.0.join("old.txt"), "old").unwrap();
    fs::set_permissions(d.0.join("old.txt"), fs::Permissions::from_mode(0o666)).unwrap();
    fs::write(d.0.join("keep.txt"), "old").unwrap();
    fs::set_permissions(d.0.join("keep.txt"), fs::Permissions::from_mode(0o664)).unwrap();
    let (_, err, code) = run(
        &d,
        "fs.write(\"old.txt\", \"new\", 0o600).unwrap()\nfs.write(\"keep.txt\", \"new\").unwrap()\n",
    );
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(d.mode("old.txt"), 0o600);
    assert_eq!(fs::read_to_string(d.0.join("old.txt")).unwrap(), "new");
    assert_eq!(d.mode("keep.txt"), 0o664);
}

#[test]
fn chmod_and_mode_change_and_read_the_bits() {
    let d = Dir::new("chmod");
    let (out, err, code) = run(
        &d,
        "fs.write(\"f\", \"x\", mode: 0o600).unwrap()\nprint(fs.mode(\"f\").unwrap() == 0o600)\nfs.chmod(\"f\", 0o644).unwrap()\nprint(fs.mode(\"f\").unwrap() == 0o644)\nprint(fs.chmod(\"f\", 99999).is_err())\nprint(fs.chmod(\"absent\", 0o600).is_err())\nprint(fs.mode(\"absent\").is_err())\nprint(fs.write(\"g\", \"x\", 0o17777).is_err())\n",
    );
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(out, "true\ntrue\ntrue\ntrue\ntrue\ntrue\n");
    assert_eq!(d.mode("f"), 0o644);
}

#[test]
fn a_copy_keeps_the_mode_of_the_source() {
    let d = Dir::new("copy");
    let (_, err, code) = run(
        &d,
        "fs.write(\"a.txt\", \"x\", mode: 0o600).unwrap()\nfs.copy(\"a.txt\", \"b.txt\").unwrap()\n",
    );
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(d.mode("b.txt"), 0o600);
}
