//! `qlang doc`: Markdown documentation from the `--" ... --"` comments of `.q` files.
//!
//! It documents what a file shows to others: its exported names (all of them when the file
//! has no `export`), and the `public` fields and methods of its types. The compiler already
//! knows every declaration with its signature and comment (`CompileOptions::info`); this
//! only lays it out.

use qlang_core::check::{CompileOptions, ItemInfo, compile_with};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SKIPPED_DIRS: [&str; 4] = ["target", "node_modules", "dist", "examples"];

fn is_test_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.ends_with("_test.q") || name.starts_with("test_")
}

/// The `.q` files below `dir` that are not tests, sorted.
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
        } else if name.ends_with(".q") && !is_test_file(&p) {
            out.push(p);
        }
    }
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut out_file: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-o" | "--out" => {
                i += 1;
                match args.get(i) {
                    Some(f) => out_file = Some(PathBuf::from(f)),
                    None => return usage("-o needs a file"),
                }
            }
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
        i += 1;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut files: Vec<PathBuf> = Vec::new();
    if paths.is_empty() {
        find(&crate::project_dir(&cwd).unwrap_or(cwd.clone()), &mut files);
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
    let canonical_cwd = cwd.canonicalize().ok();
    let mut markdown = String::new();
    let mut failed = false;
    for file in &files {
        let shown = file
            .canonicalize()
            .ok()
            .and_then(|f| {
                canonical_cwd
                    .as_ref()
                    .and_then(|c| f.strip_prefix(c).ok().map(Path::to_path_buf))
            })
            .unwrap_or_else(|| file.clone())
            .to_string_lossy()
            .replace('\\', "/");
        match document(file, &shown) {
            Ok(text) => markdown.push_str(&text),
            Err(e) => {
                eprintln!("{e}");
                failed = true;
            }
        }
    }
    if files.is_empty() {
        eprintln!("no .q files to document");
        return ExitCode::from(66);
    }
    match out_file {
        Some(f) => {
            if let Err(e) = std::fs::write(&f, &markdown) {
                eprintln!("cannot write {}: {e}", f.display());
                return ExitCode::from(1);
            }
        }
        None => print!("{markdown}"),
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn usage(why: &str) -> ExitCode {
    eprintln!("{why}\nusage: qlang doc [path...] [-o FILE]");
    ExitCode::from(64)
}

/// The Markdown section of one file.
fn document(file: &Path, shown: &str) -> Result<String, String> {
    let source = std::fs::read_to_string(file).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let mut host = crate::new_host_with(file, None, Some(0), Vec::new(), &[], true, false)?;
    let program = compile_with(shown, &source, &mut host, &CompileOptions { info: true })
        .map_err(|e| format!("{shown} does not compile:\n{}", e.render()))?;
    let info = program.info.ok_or("no information from the compiler")?;
    let items: Vec<&ItemInfo> = info
        .items
        .iter()
        .filter(|it| it.loc.file == shown && it.public)
        .collect();
    Ok(render(shown, &items))
}

fn is_top_level(it: &ItemInfo) -> bool {
    it.owner.is_none()
}

fn render(file: &str, items: &[&ItemInfo]) -> String {
    let mut out = format!("# {file}\n\n");
    let tops: Vec<&&ItemInfo> = items.iter().filter(|it| is_top_level(it)).collect();
    if tops.is_empty() {
        out.push_str("_Nothing documented here._\n\n");
    }
    for top in tops {
        out.push_str(&format!("## `{}`\n\n", top.label));
        if let Some(doc) = &top.doc {
            out.push_str(doc.trim());
            out.push_str("\n\n");
        }
        // methods of a type can be declared far from it: match on the owner's name
        for (kind, title) in [
            ("variant", "Variants"),
            ("field", "Fields"),
            ("method", "Methods"),
        ] {
            let members: Vec<&&ItemInfo> = items
                .iter()
                .filter(|m| m.kind == kind && m.owner.as_deref() == Some(top.name.as_str()))
                .collect();
            if members.is_empty() {
                continue;
            }
            out.push_str(&format!("**{title}**\n\n"));
            for m in members {
                out.push_str(&format!("- `{}`", m.label));
                if let Some(doc) = &m.doc {
                    out.push_str(" — ");
                    out.push_str(&doc.trim().replace('\n', " "));
                }
                out.push('\n');
            }
            out.push('\n');
        }
    }
    out
}
