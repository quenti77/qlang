//! `qlang fmt`: lays programs out (see `qlang_core::fmt`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SKIPPED_DIRS: [&str; 3] = ["target", "node_modules", "dist"];

fn find(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if p.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                find(&p, out);
            }
        } else if name.ends_with(".q") {
            out.push(p);
        }
    }
}

fn usage(why: &str) -> ExitCode {
    eprintln!("qlang fmt: {why}\nusage: qlang fmt [path...] [--check]");
    ExitCode::from(64)
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut check = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    for a in args {
        match *a {
            "--check" => check = true,
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    if paths.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        find(&crate::project_dir(&cwd).unwrap_or(cwd), &mut files);
    }
    for p in paths {
        if p.is_dir() {
            find(&p, &mut files);
        } else if p.is_file() {
            files.push(p);
        } else {
            eprintln!("{}: no such file or directory", p.display());
            return ExitCode::from(66);
        }
    }
    let mut changed = 0usize;
    let mut failed = false;
    for f in &files {
        let Ok(src) = std::fs::read_to_string(f) else {
            eprintln!("{}: cannot be read", f.display());
            failed = true;
            continue;
        };
        match qlang_core::fmt::format(&src) {
            Err(e) => {
                eprintln!("{}: {e}", f.display());
                failed = true;
            }
            Ok(out) if out == src => {}
            Ok(out) => {
                changed += 1;
                if check {
                    println!("{}", f.display());
                } else if let Err(e) = std::fs::write(f, out) {
                    eprintln!("{}: {e}", f.display());
                    failed = true;
                } else {
                    println!("formatted {}", f.display());
                }
            }
        }
    }
    if failed {
        ExitCode::from(65)
    } else if check && changed > 0 {
        eprintln!("{changed} file(s) would change");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
