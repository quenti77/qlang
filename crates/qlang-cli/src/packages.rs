//! Packages: `import x from "@name/file.q"` and `import "@name" as m`.
//!
//! The dependencies of a project are listed in its `qlang.toml`:
//!
//! ```toml
//! [package]
//! name = "app"
//! main = "lib.q"            # the file `import "@app"` loads (default: lib.q)
//!
//! [dependencies]
//! utils = { path = "../utils" }
//! ```
//!
//! A package is a directory with its own `qlang.toml` (which may list dependencies too:
//! they all live in one flat namespace, a name meaning the same directory everywhere).
//! The language only knows the virtual path `@name/file.q`; this module turns it into a
//! real file, and never outside the package's directory.

use qlang_core::span::{split_package_path, valid_package_name};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Package {
    pub dir: PathBuf,
    /// The file `@name` stands for, relative to `dir`.
    pub main: String,
}

/// The packages a program can import, by name.
#[derive(Clone, Debug, Default)]
pub struct Packages {
    by_name: HashMap<String, Package>,
}

fn read_manifest(dir: &Path) -> Result<Option<toml::Table>, String> {
    let file = dir.join("qlang.toml");
    if !file.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    text.parse::<toml::Table>()
        .map(Some)
        .map_err(|e| format!("{}: {e}", file.display()))
}

fn main_file(manifest: &Option<toml::Table>) -> String {
    manifest
        .as_ref()
        .and_then(|m| m.get("package"))
        .and_then(|p| p.get("main"))
        .and_then(|m| m.as_str())
        .unwrap_or("lib.q")
        .to_string()
}

impl Packages {
    /// The dependencies declared by the `qlang.toml` in `project_dir`, and recursively by
    /// the packages themselves.
    pub fn load(project_dir: &Path) -> Result<Packages, String> {
        let mut packages = Packages::default();
        let manifest = read_manifest(project_dir)?;
        if let Some(m) = &manifest {
            packages.add_dependencies(m, project_dir, "this project")?;
        }
        Ok(packages)
    }

    fn add_dependencies(
        &mut self,
        manifest: &toml::Table,
        base: &Path,
        owner: &str,
    ) -> Result<(), String> {
        let Some(deps) = manifest.get("dependencies") else {
            return Ok(());
        };
        let deps = deps
            .as_table()
            .ok_or_else(|| format!("{owner}: `dependencies` must be a table"))?;
        for (name, spec) in deps {
            if !valid_package_name(name) {
                return Err(format!(
                    "{owner}: `{name}` is not a valid package name (letters, digits, `_` and `-`)"
                ));
            }
            let spec = spec.as_table().ok_or_else(|| {
                format!(
                    "{owner}: dependency `{name}` must be a table like {{ path = \"../{name}\" }}"
                )
            })?;
            let dir = match (spec.get("path"), spec.get("git")) {
                (Some(path), None) => {
                    let path = path
                        .as_str()
                        .ok_or_else(|| format!("{owner}: `{name}`: `path` must be a string"))?;
                    let dir = base.join(path);
                    dir.canonicalize().map_err(|_| {
                        format!(
                            "{owner}: dependency `{name}`: the directory {} does not exist",
                            dir.display()
                        )
                    })?
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "{owner}: dependency `{name}`: git dependencies are not supported yet"
                    ));
                }
                _ => {
                    return Err(format!(
                        "{owner}: dependency `{name}` needs a `path` (or a `git` URL)"
                    ));
                }
            };
            if let Some(known) = self.by_name.get(name) {
                if known.dir == dir {
                    continue;
                }
                return Err(format!(
                    "{owner}: dependency `{name}` is {} here, but {} elsewhere (one version of each package per project)",
                    dir.display(),
                    known.dir.display()
                ));
            }
            let manifest = read_manifest(&dir)?;
            self.by_name.insert(
                name.clone(),
                Package {
                    dir: dir.clone(),
                    main: main_file(&manifest),
                },
            );
            if let Some(m) = &manifest {
                self.add_dependencies(m, &dir, &format!("package `{name}`"))?;
            }
        }
        Ok(())
    }

    /// `@name` as `@name/<main file of the package>`; other module paths unchanged.
    pub fn with_main(&self, path: &str) -> String {
        match split_package_path(path) {
            Some((name, "")) => match self.by_name.get(name) {
                Some(p) => format!("@{name}/{}", p.main.trim_start_matches("./")),
                None => path.to_string(),
            },
            _ => path.to_string(),
        }
    }

    /// The real file behind a module path `@name` or `@name/dir/file.q`.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let (name, rest) = split_package_path(path).ok_or("not a package path")?;
        let Some(package) = self.by_name.get(name) else {
            return Err(format!(
                "package `{name}` is not a dependency of this project (add it to `[dependencies]` in qlang.toml)"
            ));
        };
        let file = if rest.is_empty() { &package.main } else { rest };
        let full = package
            .dir
            .join(file)
            .canonicalize()
            .map_err(|_| format!("file \"{path}\" not found"))?;
        if !full.starts_with(&package.dir) {
            return Err(format!("\"{path}\" leaves its package"));
        }
        Ok(full)
    }
}
