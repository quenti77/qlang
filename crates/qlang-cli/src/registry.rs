//! `qlang publish` (is this package ready to be listed?) and `qlang verify` (are the installed packages
//! exactly what `qlang.lock` says?).

use qlang_core::check::{CompileOptions, compile_with};
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// What a check found: a line, and whether it is a problem.
struct Report {
    lines: Vec<(bool, String)>,
}

impl Report {
    fn ok(&mut self, text: impl Into<String>) {
        self.lines.push((true, text.into()));
    }

    fn bad(&mut self, text: impl Into<String>) {
        self.lines.push((false, text.into()));
    }

    fn failed(&self) -> bool {
        self.lines.iter().any(|(ok, _)| !ok)
    }

    fn print(&self) {
        for (ok, text) in &self.lines {
            println!("{} {text}", if *ok { "ok  " } else { "FAIL" });
        }
    }
}

pub fn publish(args: &[&str]) -> ExitCode {
    if let Some(a) = args.first() {
        eprintln!("qlang publish: unexpected `{a}`\nusage: qlang publish");
        return ExitCode::from(64);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let Some(dir) = crate::project_dir(&cwd) else {
        eprintln!("no qlang.toml here or in a parent directory");
        return ExitCode::from(1);
    };
    let manifest = match crate::packages::read_manifest(&dir) {
        Ok(Some(m)) => m,
        Ok(None) => {
            eprintln!("no qlang.toml in {}", dir.display());
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let package = manifest.get("package").and_then(|p| p.as_table());
    let field = |k: &str| {
        package
            .and_then(|p| p.get(k))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let mut r = Report { lines: Vec::new() };

    let name = field("name");
    match name {
        Some(n) if qlang_core::span::valid_package_name(n) => r.ok(format!("name: {n}")),
        Some(n) => r.bad(format!(
            "name `{n}` is not valid (letters, digits, `_` and `-`)"
        )),
        None => r.bad("[package] has no `name`"),
    }
    let version = field("version");
    match version {
        Some(v) => match crate::semver::Version::parse(v) {
            Some(parsed) if parsed.pre.is_none() => r.ok(format!("version: {v}")),
            Some(_) => r.bad(format!("version `{v}` is a pre-release: list a release")),
            None => r.bad(format!("version `{v}` is not X.Y.Z")),
        },
        None => r.bad("[package] has no `version`"),
    }
    let description = field("description");
    match description {
        Some(d) => r.ok(format!("description: {d}")),
        None => r.bad("[package] has no `description` (one line, shown by `qlang search`)"),
    }
    let license = field("license");
    match license {
        Some(l) => r.ok(format!("license: {l}")),
        None => r.bad("[package] has no `license` (an SPDX name, like MIT)"),
    }
    let has_readme = ["README.md", "README", "readme.md"]
        .iter()
        .any(|f| std::fs::metadata(dir.join(f)).is_ok_and(|m| m.len() > 0));
    if has_readme {
        r.ok("README present");
    } else {
        r.bad("no README.md (what the package is for, one example)");
    }
    let main = field("main").unwrap_or("lib.q").to_string();
    let main_path = dir.join(&main);
    if main_path.is_file() {
        match check_compiles(&main_path, &main) {
            Ok(()) => r.ok(format!("{main} compiles")),
            Err(e) => r.bad(format!("{main} does not compile:\n{e}")),
        }
    } else {
        r.bad(format!("the main file {main} does not exist"));
    }

    // git: committed, tagged, and where it can be fetched from
    let in_git = git_out(&dir, &["rev-parse", "--is-inside-work-tree"]).as_deref() == Some("true");
    let mut remote = None;
    if !in_git {
        r.bad("not a git repository (a package is a git repository with a tag per version)");
    } else {
        if git_out(&dir, &["status", "--porcelain"]).is_some_and(|s| s.is_empty()) {
            r.ok("working tree is clean");
        } else {
            r.bad("uncommitted changes: commit them first");
        }
        if let Some(v) = version {
            let tags = git_out(&dir, &["tag", "--points-at", "HEAD"]).unwrap_or_default();
            let wanted = [format!("v{}", v.trim_start_matches('v')), v.to_string()];
            if tags.lines().any(|t| wanted.iter().any(|w| w == t)) {
                r.ok(format!("HEAD is tagged v{}", v.trim_start_matches('v')));
            } else {
                r.bad(format!(
                    "HEAD has no tag v{0} (git tag v{0} && git push --tags)",
                    v.trim_start_matches('v')
                ));
            }
        }
        remote = git_out(&dir, &["remote", "get-url", "origin"]);
        match &remote {
            Some(u) if u.starts_with("https://") => r.ok(format!("repository: {u}")),
            Some(u) => r.bad(format!(
                "origin is {u}: the index lists an https URL anyone can clone"
            )),
            None => r.bad("no `origin` remote: push the repository somewhere first"),
        }
    }
    r.print();
    if r.failed() {
        eprintln!("\nnot ready to be listed");
        return ExitCode::from(1);
    }
    let keywords: Vec<String> = package
        .and_then(|p| p.get("keywords"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|k| k.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut entry = serde_json::Map::new();
    entry.insert(
        "git".into(),
        remote.unwrap_or_default().trim_end_matches(".git").into(),
    );
    entry.insert("description".into(), description.unwrap_or("").into());
    entry.insert("license".into(), license.unwrap_or("").into());
    if !keywords.is_empty() {
        entry.insert("keywords".into(), keywords.into());
    }
    let one = serde_json::json!({ name.unwrap_or(""): entry });
    let text = serde_json::to_string_pretty(&one).unwrap_or_default();
    // the entry is the inside of the object: drop its outer braces
    let inner: Vec<&str> = text.lines().collect();
    println!(
        "\nReady. To list it, add this entry to packages.json in https://github.com/quenti77/qlang and open a pull request:\n"
    );
    println!("{}", inner[1..inner.len() - 1].join("\n"));
    ExitCode::SUCCESS
}

fn check_compiles(file: &Path, shown: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let mut host = crate::new_host_with(file, None, Some(0), Vec::new(), &[], true, false)?;
    compile_with(shown, &source, &mut host, &CompileOptions::default())
        .map(|_| ())
        .map_err(|e| e.render())
}

/// `qlang verify`: every package of `qlang.lock` is installed, at its commit, with no file changed.
pub fn verify() -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let Some(dir) = crate::project_dir(&cwd) else {
        eprintln!("no qlang.toml here or in a parent directory");
        return ExitCode::from(1);
    };
    let lock = match std::fs::read_to_string(dir.join("qlang.lock")) {
        Ok(t) => t,
        Err(_) => {
            println!("no qlang.lock: nothing to verify");
            return ExitCode::SUCCESS;
        }
    };
    let table: toml::Table = match lock.parse() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("qlang.lock: {e}");
            return ExitCode::from(1);
        }
    };
    let mut bad = 0;
    for entry in table
        .get("package")
        .and_then(|p| p.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let get = |k: &str| entry.get(k).and_then(|v| v.as_str()).unwrap_or("");
        let (name, commit) = (get("name"), get("commit"));
        let at = crate::packages::packages_dir(&dir).join(name);
        if !at.join(".git").exists() {
            println!("FAIL {name}: not installed (run `qlang install`)");
            bad += 1;
            continue;
        }
        let head = git_out(&at, &["rev-parse", "HEAD"]).unwrap_or_default();
        if head != commit {
            println!(
                "FAIL {name}: at {} but the lock says {}",
                short(&head),
                short(commit)
            );
            bad += 1;
            continue;
        }
        if git_out(&at, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) {
            println!("FAIL {name}: files were changed after the install");
            bad += 1;
            continue;
        }
        println!("ok   {name} {}", short(commit));
    }
    if bad > 0 {
        eprintln!("{bad} package(s) differ from qlang.lock");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(10)]
}
