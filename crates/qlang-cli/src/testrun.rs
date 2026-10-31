//! `qlang test`: runs the `fun test_...()` of the test files.
//!
//! A test file is `something_test.q` or `test_something.q`; every top-level function of it
//! named `test_...` without parameters is a test, run on its own from a clean state. A test
//! passes unless it stops with an error (a failed `assert` / `assert_eq`, a `panic`...).

use qlang_core::check::compile;
use qlang_core::coverage::FileLines;
use qlang_core::host::Limits;
use std::collections::BTreeMap;
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
    let mut allow_process = false;
    let mut deny_warnings = false;
    let mut coverage = false;
    let mut lcov: Option<String> = None;
    let mut groups: Option<String> = None;
    let mut skip_groups: Option<String> = None;
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
            "--allow-process" => allow_process = true,
            "--deny-warnings" => deny_warnings = true,
            "--coverage" => coverage = true,
            "--coverage-lcov" => {
                i += 1;
                match args.get(i) {
                    Some(f) => {
                        coverage = true;
                        lcov = Some(f.to_string());
                    }
                    None => return usage("--coverage-lcov needs a file"),
                }
            }
            "--group" => {
                i += 1;
                match args.get(i) {
                    Some(g) => groups = Some(g.to_string()),
                    None => return usage("--group needs a tag (or tags separated by commas)"),
                }
            }
            "--skip-group" => {
                i += 1;
                match args.get(i) {
                    Some(g) => skip_groups = Some(g.to_string()),
                    None => return usage("--skip-group needs a tag (or tags separated by commas)"),
                }
            }
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

    // `qlang-test` reads these two variables to keep or leave out the groups of its cases
    // (set while the program is still on one thread)
    if let Some(g) = &groups {
        unsafe { std::env::set_var("QLANG_TEST_GROUP", g) };
    }
    if let Some(g) = &skip_groups {
        unsafe { std::env::set_var("QLANG_TEST_SKIP_GROUP", g) };
    }
    qlang_core::coverage::enable(coverage);
    let mut covered: BTreeMap<String, FileLines> = BTreeMap::new();
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
        host.allow_process = allow_process;
        let program = match compile(&shown, &source, &mut host) {
            Ok(p) => p,
            Err(e) => {
                println!("FAIL {shown}: it does not compile");
                print!("{}", indent(&e.render()));
                failed += 1;
                continue;
            }
        };
        if deny_warnings && !program.warnings.is_empty() {
            println!("FAIL {shown}: it has warnings, and --deny-warnings is on");
            for w in &program.warnings {
                print!("{}", indent(&w.render(&program.sources)));
            }
            failed += 1;
            continue;
        }
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
        if coverage {
            merge(&mut covered, qlang_core::coverage::take(&program.sources));
        }
    }
    if coverage {
        report(&covered, lcov.as_deref());
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

/// Add what one test file's run covered to the total.
fn merge(total: &mut BTreeMap<String, FileLines>, part: BTreeMap<String, FileLines>) {
    for (name, f) in part {
        let e = total.entry(name).or_default();
        e.lines.extend(f.lines);
        e.hit.extend(f.hit);
    }
}

/// The share of statement lines that ran, per source file (the test files themselves are left out),
/// and an LCOV file when asked.
fn report(covered: &BTreeMap<String, FileLines>, lcov: Option<&str>) {
    println!("\ncoverage (lines with a statement)");
    let (mut all, mut hit) = (0usize, 0usize);
    let mut out = String::new();
    for (name, f) in covered {
        if is_test_file(Path::new(name)) {
            continue;
        }
        let h = f.hit.len();
        let n = f.lines.len();
        all += n;
        hit += h;
        let pct = if n == 0 {
            100.0
        } else {
            h as f64 * 100.0 / n as f64
        };
        let missing: Vec<String> = f.lines.difference(&f.hit).map(|l| l.to_string()).collect();
        let tail = if missing.is_empty() {
            String::new()
        } else {
            format!("  not run: {}", missing.join(", "))
        };
        println!("  {pct:5.1}%  {name} ({h}/{n}){tail}");
        out.push_str(&format!("SF:{name}\n"));
        for l in &f.lines {
            out.push_str(&format!("DA:{l},{}\n", u32::from(f.hit.contains(l))));
        }
        out.push_str(&format!("LF:{n}\nLH:{h}\nend_of_record\n"));
    }
    let pct = if all == 0 {
        100.0
    } else {
        hit as f64 * 100.0 / all as f64
    };
    println!("  {pct:5.1}%  total ({hit}/{all})");
    if let Some(path) = lcov {
        match std::fs::write(path, out) {
            Ok(()) => println!("  LCOV written to {path}"),
            Err(e) => eprintln!("{path}: {e}"),
        }
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
    eprintln!(
        "{why}\nusage: qlang test [path...] [--filter TEXT] [--group TAGS] [--skip-group TAGS] [--coverage] [--coverage-lcov FILE] [--seed N] [--max-steps N]"
    );
    ExitCode::from(64)
}
