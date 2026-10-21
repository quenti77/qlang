//! `qlang test`: runs the `fun test_...()` of the test files.
//!
//! A test file is `something_test.q` or `test_something.q`; every top-level function of it
//! named `test_...` without parameters is a test, run on its own from a clean state. A test
//! passes unless it stops with an error (a failed `assert` / `assert_eq`, a `panic`...).

use qlang_core::check::compile;
use qlang_core::host::Limits;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SKIPPED_DIRS: [&str; 3] = ["target", "node_modules", "dist"];

fn is_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.ends_with(".q") && (name.ends_with("_test.q") || name.starts_with("test_"))
}

/// The test files below `dir`, sorted; hidden directories and build output are skipped.
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
        } else if is_test_file(&p) {
            out.push(p);
        }
    }
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut filter: Option<String> = None;
    let mut seed: Option<u64> = None;
    let mut limits = Limits::default();
    let mut trust = false;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--filter" => {
                i += 1;
                match args.get(i) {
                    Some(f) => filter = Some(f.to_string()),
                    None => return usage("--filter needs a text"),
                }
            }
            "--seed" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => seed = Some(n),
                    None => return usage("--seed needs a number"),
                }
            }
            "--max-steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => limits.max_steps = n,
                    None => return usage("--max-steps needs a number"),
                }
            }
            "--trust-externs" => trust = true,
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
        i += 1;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    if paths.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let base = crate::project_dir(&cwd).unwrap_or(cwd);
        find(&base, &mut files);
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

    let (mut passed, mut failed) = (0usize, 0usize);
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|c| c.canonicalize().ok());
    for file in &files {
        // shown from where the user is, when the file is below
        let shown = file
            .canonicalize()
            .ok()
            .and_then(|f| {
                cwd.as_ref()
                    .and_then(|c| f.strip_prefix(c).ok().map(Path::to_path_buf))
            })
            .unwrap_or_else(|| file.clone())
            .display()
            .to_string();
        let source = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                println!("FAIL {shown}: cannot read it: {e}");
                failed += 1;
                continue;
            }
        };
        let mut host = match crate::new_host(file, None, seed, Vec::new(), &[], trust) {
            Ok(h) => h,
            Err(e) => {
                println!("FAIL {shown}: {e}");
                failed += 1;
                continue;
            }
        };
        let program = match compile(&shown, &source, &mut host) {
            Ok(p) => p,
            Err(e) => {
                println!("FAIL {shown}: it does not compile");
                print!("{}", indent(&e.render()));
                failed += 1;
                continue;
            }
        };
        for name in program.test_names() {
            if filter.as_ref().is_some_and(|f| !name.contains(f.as_str())) {
                continue;
            }
            match program.run_test(&name, &mut host, limits_of(&limits)) {
                Ok(()) => {
                    println!("ok   {shown}::{name}");
                    passed += 1;
                }
                Err(d) => {
                    println!("FAIL {shown}::{name}");
                    print!("{}", indent(&d.render(&program.sources)));
                    failed += 1;
                }
            }
        }
    }
    if passed + failed == 0 {
        println!("no tests found (files named *_test.q or test_*.q, with `fun test_...()`)");
        return ExitCode::SUCCESS;
    }
    println!("\n{passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn limits_of(l: &Limits) -> Limits {
    Limits {
        max_steps: l.max_steps,
        max_depth: l.max_depth,
        max_output: l.max_output,
        max_alloc: l.max_alloc,
    }
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("     {l}\n")).collect()
}

fn usage(why: &str) -> ExitCode {
    eprintln!("{why}\nusage: qlang test [path...] [--filter TEXT] [--seed N] [--max-steps N]");
    ExitCode::from(64)
}
