//! `qlang migrate`: rewrites the mechanical part of a breaking change (see docs/migration.md).
//!
//! Each rule replaces a few names, found as tokens (never inside a string or a comment). The result is
//! checked before it is written: it must have exactly the tokens the file had, with only the replaced names
//! different.

use qlang_core::lexer::{Tok, Token, lex};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SKIPPED_DIRS: [&str; 3] = ["target", "node_modules", "dist"];

/// One renaming of `owner.old` into `owner.new`, announced in the version `since`.
struct Rule {
    since: &'static str,
    owner: &'static str,
    old: &'static str,
    new: &'static str,
    why: &'static str,
}

const RULES: &[Rule] = &[Rule {
    since: "0.69",
    owner: "Log",
    old: "warn",
    new: "warning",
    why: "`Log.warn` is deprecated, `Log.warning` is the name",
}];

struct Edit {
    start: usize,
    end: usize,
    text: &'static str,
    rule: &'static Rule,
}

fn find_edits(tokens: &[Token]) -> Vec<Edit> {
    let mut edits = Vec::new();
    for w in tokens.windows(3) {
        let (Tok::Ident(owner), Tok::Dot, Tok::Ident(name)) = (&w[0].tok, &w[1].tok, &w[2].tok)
        else {
            continue;
        };
        if let Some(rule) = RULES.iter().find(|r| r.owner == owner && r.old == name) {
            edits.push(Edit {
                start: w[2].span.start as usize,
                end: w[2].span.end as usize,
                text: rule.new,
                rule,
            });
        }
    }
    edits
}

fn usage(why: &str) -> ExitCode {
    eprintln!("qlang migrate: {why}\nusage: qlang migrate [path...] [--check]");
    ExitCode::from(64)
}

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

/// The text with the edits applied, or why it cannot be trusted.
fn rewrite(source: &str, edits: &[Edit]) -> Result<String, String> {
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for e in edits {
        out.push_str(&source[at..e.start]);
        out.push_str(e.text);
        at = e.end;
    }
    out.push_str(&source[at..]);
    // the same tokens, but the ones that were renamed
    let (mut d1, mut d2) = (Vec::new(), Vec::new());
    let before = lex(source, 0, 0, &mut d1);
    let after = lex(&out, 0, 0, &mut d2);
    if before.len() != after.len() || d1.len() != d2.len() {
        return Err("the result does not have the tokens of the original".into());
    }
    let renamed = |t: &Tok| -> String { format!("{t:?}") };
    let different = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| renamed(&a.tok) != renamed(&b.tok))
        .count();
    if different != edits.len() {
        return Err("more than the renamed names changed".into());
    }
    Ok(out)
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut check = false;
    let mut paths: Vec<PathBuf> = Vec::new();
    for a in args {
        match *a {
            "--check" => check = true,
            "--rules" => {
                for r in RULES {
                    println!(
                        "{}: {}.{} -> {}.{}  ({})",
                        r.since, r.owner, r.old, r.owner, r.new, r.why
                    );
                }
                return ExitCode::SUCCESS;
            }
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
    }
    let mut files = Vec::new();
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
    let (mut changed, mut failed) = (0usize, false);
    for f in &files {
        let Ok(source) = std::fs::read_to_string(f) else {
            eprintln!("{}: cannot be read", f.display());
            failed = true;
            continue;
        };
        let mut diags = Vec::new();
        let tokens = lex(&source, 0, 0, &mut diags);
        if !diags.is_empty() {
            eprintln!("{}: it does not lex, left as it is", f.display());
            failed = true;
            continue;
        }
        let edits = find_edits(&tokens);
        if edits.is_empty() {
            continue;
        }
        match rewrite(&source, &edits) {
            Err(e) => {
                eprintln!("{}: {e}, left as it is", f.display());
                failed = true;
            }
            Ok(out) => {
                changed += 1;
                let rules: Vec<String> = {
                    let mut v: Vec<String> = edits
                        .iter()
                        .map(|e| {
                            format!(
                                "{}.{} -> {}.{} (since {})",
                                e.rule.owner, e.rule.old, e.rule.owner, e.rule.new, e.rule.since
                            )
                        })
                        .collect();
                    v.dedup();
                    v
                };
                if check {
                    println!(
                        "{}: {} change(s): {}",
                        f.display(),
                        edits.len(),
                        rules.join(", ")
                    );
                } else if let Err(e) = std::fs::write(f, out) {
                    eprintln!("{}: {e}", f.display());
                    failed = true;
                } else {
                    println!(
                        "migrated {}: {} change(s): {}",
                        f.display(),
                        edits.len(),
                        rules.join(", ")
                    );
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
