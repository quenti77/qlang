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
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn read(location: &str) -> Result<String, String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        let resp = ureq::get(location)
            .timeout(std::time::Duration::from_secs(10))
            .call()
            .map_err(|e| format!("cannot read the package index: {e}"))?;
        let mut text = String::new();
        use std::io::Read;
        resp.into_reader()
            .take(1 << 20)
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read the package index: {e}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(location).map_err(|e| format!("cannot read {location}: {e}"))
    }
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

/// `1.2.3` from `v1.2.3` (or `1.2.3`); `None` for any other tag.
fn version_of(tag: &str) -> Option<(u64, u64, u64)> {
    let t = tag.strip_prefix('v').unwrap_or(tag);
    let mut it = t.split('.');
    let (a, b, c) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?, c.parse().ok()?))
}

/// The highest `vX.Y.Z` tag of a git repository.
pub fn latest_tag(git_url: &str) -> Option<String> {
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
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split("refs/tags/").nth(1))
        .filter_map(|t| version_of(t).map(|v| (v, t.to_string())))
        .max()
        .map(|(_, t)| t)
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
            println!("{:<16} {}\n{:<16} {}", e.name, e.description, "", e.git);
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
    fn tags_are_compared_as_numbers() {
        assert_eq!(version_of("v0.10.0"), Some((0, 10, 0)));
        assert!(version_of("v0.10.0") > version_of("v0.9.0"));
        assert_eq!(version_of("latest"), None);
        assert_eq!(version_of("v1.2"), None);
        assert_eq!(version_of("v1.2.3.4"), None);
    }

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
