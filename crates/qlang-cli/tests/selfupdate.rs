//! `qlang self-update` against a release served from this machine, and the `qlang = "..."`
//! requirement of a project.

#![cfg(unix)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

fn serve(files: HashMap<String, Vec<u8>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            let (status, body) = match files.get(&path) {
                Some(b) => ("200 OK", b.clone()),
                None => ("404 Not Found", b"missing".to_vec()),
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
    format!("http://{addr}")
}

fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        _ => "x86_64-apple-darwin",
    }
}

fn archive(program: &[u8]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    // like the real archives: the folder first, then the files
    let mut dir = tar::Header::new_gnu();
    dir.set_entry_type(tar::EntryType::Directory);
    dir.set_size(0);
    dir.set_mode(0o755);
    dir.set_cksum();
    tar.append_data(&mut dir, "qlang/", std::io::empty())
        .unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_size(program.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "qlang/qlang", program)
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

fn sha256(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for b in ring::digest::digest(&ring::digest::SHA256, data).as_ref() {
        write!(out, "{b:02x}").unwrap();
    }
    out
}

/// A copy of qlang in its own folder, to be replaced.
fn copy_of_qlang(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-selfupdate-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("qlang");
    std::fs::copy(env!("CARGO_BIN_EXE_qlang"), &exe).unwrap();
    exe
}

/// A "program" that answers `--version` like qlang and otherwise says its name; padded so it
/// is not mistaken for an empty file.
fn fake_program(name: &str) -> Vec<u8> {
    let mut text = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo qlang 9.9.9; exit 0; fi\necho qlang {name}\n"
    );
    text.push_str(&"#".repeat(2000));
    text.into_bytes()
}

fn release(version: &str, program: &[u8], sum_of: Option<&str>) -> HashMap<String, Vec<u8>> {
    let name = format!("qlang-{}.tar.gz", target());
    let data = archive(program);
    let sum = sum_of.map(str::to_string).unwrap_or_else(|| sha256(&data));
    let mut files = HashMap::new();
    files.insert(
        format!("/download/v{version}/SHA256SUMS"),
        format!("{sum}  {name}\n").into_bytes(),
    );
    files.insert(format!("/download/v{version}/{name}"), data);
    files.insert(
        "/api".to_string(),
        format!("{{\"tag_name\": \"v{version}\"}}").into_bytes(),
    );
    files
}

/// Runs a command; a program that was copied a moment ago may still be "busy" in a test that runs
/// at the same time (Linux), so it is tried again.
fn output(cmd: &mut Command) -> std::process::Output {
    for _ in 0..50 {
        match cmd.output() {
            Ok(o) => return o,
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Err(e) => panic!("{e}"),
        }
    }
    cmd.output().unwrap()
}

fn update(exe: &std::path::Path, base: &str, args: &[&str]) -> std::process::Output {
    output(
        Command::new(exe)
            .arg("self-update")
            .args(args)
            .env("QLANG_BASE_URL", base)
            .env("QLANG_API_URL", format!("{base}/api")),
    )
}

#[test]
fn self_update_replaces_the_program_after_checking_its_checksum() {
    let base = serve(release("9.9.9", &fake_program("nine"), None));
    let exe = copy_of_qlang("ok");
    let o = update(&exe, &base, &[]);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{text} {}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(text.contains("-> qlang 9.9.9"), "{text}");
    let out = output(&mut Command::new(&exe));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "qlang nine\n");
    assert!(std::fs::metadata(&exe).unwrap().len() > 1024);
    assert!(!exe.with_extension("old").exists() && !exe.with_extension("new").exists());
}

#[test]
fn a_wrong_checksum_leaves_the_program_alone() {
    let base = serve(release(
        "9.9.9",
        &fake_program("evil"),
        Some(&"0".repeat(64)),
    ));
    let exe = copy_of_qlang("bad");
    let o = update(&exe, &base, &[]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("checksum mismatch"));
    let version = output(Command::new(&exe).arg("--version"));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("qlang 0."));
}

#[test]
fn check_only_says_what_exists() {
    let base = serve(release("9.9.9", &fake_program("x"), None));
    let exe = copy_of_qlang("check");
    let o = update(&exe, &base, &["--check"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("qlang 9.9.9 is available"));
    let version = output(Command::new(&exe).arg("--version"));
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("qlang 0."));
}

#[test]
fn a_project_can_ask_for_a_newer_qlang() {
    let dir = std::env::temp_dir().join(format!("qlang-require-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.q"), "print(1)\n").unwrap();
    let run = |req: &str| {
        std::fs::write(
            dir.join("qlang.toml"),
            format!("[package]\nname = \"p\"\nversion = \"0.1.0\"\nentry = \"main.q\"\nqlang = \"{req}\"\n"),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_qlang"))
            .args(["run", "main.q"])
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let ok = run("0.1");
    assert_eq!(String::from_utf8_lossy(&ok.stdout).trim(), "1");
    let too_new = run("99.0");
    assert!(!too_new.status.success());
    assert!(String::from_utf8_lossy(&too_new.stderr).contains("this project needs qlang 99.0"));
}
