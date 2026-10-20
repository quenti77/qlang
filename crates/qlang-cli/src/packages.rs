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

/// Where a dependency comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A directory, used where it is.
    Path(PathBuf),
    Git {
        url: String,
        spec: GitRef,
    },
}

/// Which commit of a repository a dependency wants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitRef {
    /// The repository's default branch.
    Default,
    Tag(String),
    Rev(String),
    Branch(String),
}

impl GitRef {
    /// How the lock file writes it (`tag:v1.2.0`).
    pub fn label(&self) -> String {
        match self {
            GitRef::Default => "default".to_string(),
            GitRef::Tag(t) => format!("tag:{t}"),
            GitRef::Rev(r) => format!("rev:{r}"),
            GitRef::Branch(b) => format!("branch:{b}"),
        }
    }
}

/// The dependencies a `qlang.toml` lists, in file order. `base` is the directory `path`
/// dependencies are relative to; `owner` names the manifest in messages.
pub fn parse_dependencies(
    manifest: &toml::Table,
    base: &Path,
    owner: &str,
) -> Result<Vec<(String, Source)>, String> {
    let Some(deps) = manifest.get("dependencies") else {
        return Ok(Vec::new());
    };
    let deps = deps
        .as_table()
        .ok_or_else(|| format!("{owner}: `dependencies` must be a table"))?;
    let mut out = Vec::new();
    for (name, spec) in deps {
        if !valid_package_name(name) {
            return Err(format!(
                "{owner}: `{name}` is not a valid package name (letters, digits, `_` and `-`)"
            ));
        }
        let spec = spec.as_table().ok_or_else(|| {
            format!("{owner}: dependency `{name}` must be a table like {{ path = \"../{name}\" }}")
        })?;
        let text = |key: &str| -> Result<Option<String>, String> {
            match spec.get(key) {
                None => Ok(None),
                Some(v) => v
                    .as_str()
                    .map(|s| Some(s.to_string()))
                    .ok_or_else(|| format!("{owner}: `{name}`: `{key}` must be a string")),
            }
        };
        let source = match (text("path")?, text("git")?) {
            (Some(path), None) => Source::Path(base.join(path)),
            (None, Some(url)) => {
                let refs: Vec<GitRef> = [
                    text("tag")?.map(GitRef::Tag),
                    text("rev")?.map(GitRef::Rev),
                    text("branch")?.map(GitRef::Branch),
                ]
                .into_iter()
                .flatten()
                .collect();
                if refs.len() > 1 {
                    return Err(format!(
                        "{owner}: dependency `{name}`: choose one of `tag`, `rev` and `branch`"
                    ));
                }
                Source::Git {
                    url,
                    spec: refs.into_iter().next().unwrap_or(GitRef::Default),
                }
            }
            _ => {
                return Err(format!(
                    "{owner}: dependency `{name}` needs a `path` or a `git` URL (not both)"
                ));
            }
        };
        out.push((name.clone(), source));
    }
    Ok(out)
}

/// Where installed (git) packages live, below the project directory.
pub fn packages_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".qlang").join("packages")
}

pub fn manifest_of(dir: &Path) -> Result<Option<toml::Table>, String> {
    read_manifest(dir)
}

impl Packages {
    /// The dependencies declared by the `qlang.toml` in `project_dir`, and recursively by
    /// the packages themselves. Git packages are expected in `.qlang/packages` (see
    /// `qlang install`).
    pub fn load(project_dir: &Path) -> Result<Packages, String> {
        let mut packages = Packages::default();
        let manifest = read_manifest(project_dir)?;
        if let Some(m) = &manifest {
            let deps = parse_dependencies(m, project_dir, "this project")?;
            packages.add_all(deps, project_dir, "this project", true)?;
        }
        Ok(packages)
    }

    fn add_all(
        &mut self,
        deps: Vec<(String, Source)>,
        project_dir: &Path,
        owner: &str,
        allow_path: bool,
    ) -> Result<(), String> {
        for (name, source) in deps {
            let dir = match &source {
                Source::Path(dir) if allow_path => dir.canonicalize().map_err(|_| {
                    format!(
                        "{owner}: dependency `{name}`: the directory {} does not exist",
                        dir.display()
                    )
                })?,
                Source::Path(_) => {
                    return Err(format!(
                        "{owner}: dependency `{name}`: a package fetched from git cannot depend on a local path"
                    ));
                }
                Source::Git { .. } => {
                    let dir = packages_dir(project_dir).join(&name);
                    dir.canonicalize().map_err(|_| {
                        format!("{owner}: package `{name}` is not installed (run `qlang install`)")
                    })?
                }
            };
            if let Some(known) = self.by_name.get(&name) {
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
                let owner = format!("package `{name}`");
                let nested = parse_dependencies(m, &dir, &owner)?;
                // inside a package, `path` dependencies are only allowed for packages that
                // are themselves local
                let local = matches!(source, Source::Path(_));
                self.add_all(nested, project_dir, &owner, local)?;
            }
        }
        Ok(())
    }

    /// `{"name": {"dir": ..., "main": ...}}`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.by_name
                .iter()
                .map(|(name, p)| {
                    (
                        name.clone(),
                        serde_json::json!({ "dir": p.dir.to_string_lossy(), "main": p.main }),
                    )
                })
                .collect(),
        )
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
