//! The package index: `qlang add std` finds where `std` lives, and `qlang search` lists what is known.
//!
//! The index is a small JSON file, `{"name": {"git": URL, "description": TEXT}}`, read from
//! `$QLANG_INDEX_URL` (a URL or a file path) or, by default, from `packages.json` in the qlang
//! repository. Offline, or when that file cannot be read, the packages built into this binary are used.
//! Nothing is downloaded or run because of the index: it only tells which git URL a name stands
//! for, and the package is then fetched and pinned like any other (`qlang.lock`).

use std::process::{Command, Stdio};

const DEFAULT_URL: &str = "https://raw.githubusercontent.com/quenti77/qlang/main/packages.json";

const BUILT_IN: &str = r#"{"std": {"git": "https://github.com/quenti77/qlang-std", "description": "Web framework pieces, queue, mail, S3, scheduler, JWT, CSV, TOML, templates, CLI helpers"}}"#;

pub struct Entry {
    pub name: String,
    pub git: String,
    pub description: String,
    pub license: String,
    pub keywords: Vec<String>,
    /// Version tags that must not be chosen any more (a flaw was found): installs that already have
    /// them locked keep working, a new resolution skips them.
    pub yanked: Vec<String>,
    /// Set when the package is withdrawn from the index: why. `qlang add` refuses it.
    pub removed: Option<String>,
}

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the package index is not JSON: {e}"))?;
    let obj = json
        .as_object()
        .ok_or("the package index must be a JSON object")?;
    let mut out = Vec::new();
    for (name, v) in obj {
        let Some(git) = v["git"].as_str() else {
            continue;
        };
        if !qlang_core::span::valid_package_name(name) {
            continue;
        }
        out.push(Entry {
            name: name.clone(),
            git: git.to_string(),
            description: v["description"].as_str().unwrap_or("").to_string(),
            license: v["license"].as_str().unwrap_or("").to_string(),
            keywords: strings(&v["keywords"]),
            yanked: strings(&v["yanked"]),
            removed: v["removed"].as_str().map(str::to_string),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn read(location: &str) -> Result<String, String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .into();
        let resp = agent
            .get(location)
            .call()
            .map_err(|e| format!("cannot read the package index: {e}"))?;
        let mut text = String::new();
        use std::io::Read;
        resp.into_body()
            .into_reader()
            .take(1 << 20)
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read the package index: {e}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(location).map_err(|e| format!("cannot read {location}: {e}"))
    }
}

/// The yanked versions of the package at `git_url`, if the index knows it (read once).
pub fn yanked_for(git_url: &str) -> Vec<String> {
    static ENTRIES: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    ENTRIES
        .get_or_init(entries)
        .iter()
        .find(|e| {
            e.git.trim_end_matches('/').trim_end_matches(".git")
                == git_url.trim_end_matches('/').trim_end_matches(".git")
        })
        .map(|e| e.yanked.clone())
        .unwrap_or_default()
}

/// Every package the index knows (the built-in ones when it cannot be read).
pub fn entries() -> Vec<Entry> {
    let location = std::env::var("QLANG_INDEX_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    match read(&location).and_then(|t| parse(&t)) {
        Ok(list) if !list.is_empty() => list,
        _ => parse(BUILT_IN).unwrap_or_default(),
    }
}

pub fn find(name: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.name == name)
}

/// Every version tag (`vX.Y.Z`, optionally with a pre-release) of a git repository, highest first.
pub fn tags(git_url: &str) -> Option<Vec<String>> {
    let out = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", git_url])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "file:https:ssh:git")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut found: Vec<(crate::semver::Version, String)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split("refs/tags/").nth(1))
        .filter_map(|t| crate::semver::Version::parse(t).map(|v| (v, t.to_string())))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    Some(found.into_iter().map(|(_, t)| t).collect())
}

/// The highest release tag (no pre-release) of a git repository.
pub fn latest_tag(git_url: &str) -> Option<String> {
    tags(git_url)?
        .into_iter()
        .find(|t| crate::semver::Version::parse(t).is_some_and(|v| v.pre.is_none()))
}

/// `qlang info <name>`.
pub fn info(name: Option<&str>) -> std::process::ExitCode {
    let Some(name) = name else {
        eprintln!("usage: qlang info <name>");
        return std::process::ExitCode::from(64);
    };
    let Some(e) = find(name) else {
        eprintln!("`{name}` is not in the package index (see `qlang search`)");
        return std::process::ExitCode::from(1);
    };
    println!("{}  {}", e.name, e.description);
    println!("repository: {}", e.git);
    println!(
        "license:    {}",
        if e.license.is_empty() {
            "(not said)"
        } else {
            &e.license
        }
    );
    if !e.keywords.is_empty() {
        println!("keywords:   {}", e.keywords.join(", "));
    }
    if let Some(why) = &e.removed {
        println!("REMOVED from the index: {why}");
    }
    match tags(&e.git) {
        Some(t) if !t.is_empty() => {
            let shown: Vec<String> = t
                .iter()
                .take(8)
                .map(|v| {
                    if e.yanked.contains(v) {
                        format!("{v} (yanked)")
                    } else {
                        v.clone()
                    }
                })
                .collect();
            let more = if t.len() > 8 {
                format!(", and {} older", t.len() - 8)
            } else {
                String::new()
            };
            println!("versions:   {}{more}", shown.join(", "));
            if let Some(latest) = latest_tag(&e.git) {
                println!(
                    "add it:     qlang add {} --git {} --version \"^{}\"",
                    e.name,
                    e.git,
                    latest.trim_start_matches('v')
                );
            }
        }
        _ => println!("versions:   (cannot reach the repository, or it has no version tag)"),
    }
    std::process::ExitCode::SUCCESS
}

/// `qlang search [text]`.
pub fn search(text: Option<&str>) -> std::process::ExitCode {
    let wanted = text.unwrap_or("").to_lowercase();
    let mut shown = 0;
    for e in entries() {
        if wanted.is_empty()
            || e.name.to_lowercase().contains(&wanted)
            || e.description.to_lowercase().contains(&wanted)
        {
            if e.removed.is_some() {
                continue;
            }
            let license = if e.license.is_empty() {
                String::new()
            } else {
                format!("  [{}]", e.license)
            };
            println!(
                "{:<16} {}{license}\n{:<16} {}",
                e.name, e.description, "", e.git
            );
            shown += 1;
        }
    }
    if shown == 0 {
        eprintln!("no package matches");
        return std::process::ExitCode::from(1);
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_is_read_and_bad_entries_are_skipped() {
        let list = parse(r#"{"b": {"git": "https://x/b"}, "a": {"git": "https://x/a", "description": "A"}, "no git": {"git": "u"}, "c": {}}"#).unwrap();
        assert_eq!(
            list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(list[0].description, "A");
        assert!(parse("[]").is_err());
        assert!(!parse(BUILT_IN).unwrap().is_empty());
    }
}
