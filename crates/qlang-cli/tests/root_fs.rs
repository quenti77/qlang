//! `--root` keeps `fs` inside a folder: `..`, an absolute path and a link to the outside all fail.

use std::process::Command;

#[test]
fn fs_stays_inside_the_root() {
    let base = std::env::temp_dir().join(format!("qlang-root-fs-{}", std::process::id()));
    let root = base.join("root");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(base.join("secret.txt"), "secret").unwrap();
    std::fs::write(root.join("ok.txt"), "ok").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(base.join("secret.txt"), root.join("link.txt")).unwrap();
    std::fs::write(
        root.join("main.q"),
        r#"for p in ["ok.txt", "sub/../ok.txt", "../secret.txt", "sub/../../secret.txt", "link.txt", "/etc/hostname"] do
  print("{p} {fs.read(p).is_ok()}")
end
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .current_dir(&root)
        .args(["run", "--root", ".", "main.q"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("ok.txt true"), "{text}");
    assert!(text.contains("sub/../ok.txt true"), "{text}");
    assert!(text.contains("../secret.txt false"), "{text}");
    assert!(text.contains("sub/../../secret.txt false"), "{text}");
    assert!(text.contains("/etc/hostname false"), "{text}");
    #[cfg(unix)]
    assert!(text.contains("link.txt false"), "{text}");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn create_dir_and_exists_stay_inside_the_root() {
    let base = std::env::temp_dir().join(format!("qlang-root-dir-{}", std::process::id()));
    let root = base.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("main.q"),
        r#"print(fs.exists("up").unwrap())
print(fs.create_dir("up/a/b").is_ok())
print(fs.create_dir("up/a/b").is_ok())
print(fs.exists("up/a/b").unwrap())
print(fs.write_bytes("up/a/b/f.bin", "x".to_bytes()).is_ok())
print(fs.create_dir("../outside").is_ok())
print(fs.create_dir("up/new/../../../outside2").is_ok())
print(fs.create_dir("/tmp/qlang-should-not-exist-9z").is_ok())
print(fs.exists("..").is_ok())
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .current_dir(&root)
        .args(["run", "--root", ".", "main.q"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "false\ntrue\ntrue\ntrue\ntrue\nfalse\nfalse\nfalse\nfalse\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!base.join("outside").exists() && !base.join("outside2").exists());
    assert!(!std::path::Path::new("/tmp/qlang-should-not-exist-9z").exists());
    let _ = std::fs::remove_dir_all(&base);
}
