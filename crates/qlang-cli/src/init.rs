//! `qlang init [dir]`: the files of a new project.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAIN: &str = "print(\"Hello, qlang!\")\n";
const GITIGNORE: &str = ".qlang/\n";

/// A package name made of letters, digits, `_` and `-` from a directory name.
fn package_name(dir: &Path) -> String {
    let raw = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-').to_string();
    if name.is_empty() {
        "app".to_string()
    } else {
        name
    }
}

pub fn init(args: &[&str]) -> ExitCode {
    let dir = match args {
        [] => PathBuf::from("."),
        [dir] if !dir.starts_with('-') => PathBuf::from(dir),
        _ => {
            eprintln!("usage: qlang init [dir]");
            return ExitCode::from(64);
        }
    };
    match create(&dir) {
        Ok(created) => {
            for f in created {
                println!("created {f}");
            }
            println!("run it with: qlang run");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

/// Writes the project files into `dir` (made if missing) and returns what it created. An
/// existing `qlang.toml` means there already is a project: nothing is touched.
fn create(dir: &Path) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let manifest = dir.join("qlang.toml");
    if manifest.exists() {
        return Err(format!(
            "{} already exists: this is already a project",
            manifest.display()
        ));
    }
    let name = package_name(&dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
    let mut created = Vec::new();
    let mut write = |file: &str, text: &str| -> Result<(), String> {
        let path = dir.join(file);
        if path.exists() {
            return Ok(()); // never overwrite what the user already has
        }
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        created.push(path.display().to_string());
        Ok(())
    };
    write(
        "qlang.toml",
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"main.q\"\n"),
    )?;
    write("main.q", MAIN)?;
    write(".gitignore", GITIGNORE)?;
    Ok(created)
}
