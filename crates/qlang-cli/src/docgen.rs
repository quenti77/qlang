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

/// `[stability]` of `qlang.toml`: the level of each file, by glob (`provisional = ["csv.q", "web/*.q"]`).
struct Levels(Vec<(&'static str, Vec<String>)>);

impl Levels {
    fn load(dir: &Path) -> Levels {
        let table = match crate::packages::read_manifest(dir) {
            Ok(Some(m)) => m.get("stability").and_then(|s| s.as_table().cloned()),
            _ => None,
        };
        let mut out = Vec::new();
        for level in ["stable", "provisional", "experimental"] {
            let patterns: Vec<String> = table
                .as_ref()
                .and_then(|t| t.get(level))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            out.push((level, patterns));
        }
        Levels(out)
    }

    /// The first level whose patterns match, in the order stable, provisional, experimental... except that
    /// an exact name wins over a pattern, so that `web/*.q = provisional` can have `web/ws.q = experimental`.
    fn of(&self, file: &str) -> Option<&'static str> {
        let exact = self
            .0
            .iter()
            .find(|(_, ps)| ps.iter().any(|p| p == file))
            .map(|(l, _)| *l);
        exact.or_else(|| {
            self.0
                .iter()
                .find(|(_, ps)| ps.iter().any(|p| glob_match(p, file)))
                .map(|(l, _)| *l)
        })
    }
}

/// `*` (anything but `/`), `**` (anything), `?` (one character).
fn glob_match(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') if p.get(1) == Some(&b'*') => {
                let rest = &p[2..];
                (0..=t.len()).any(|i| go(rest, &t[i..]))
            }
            Some(b'*') => {
                let rest = &p[1..];
                (0..=t.len()).any(|i| go(rest, &t[i..]) && !t[..i].contains(&b'/'))
            }
            Some(b'?') => !t.is_empty() && t[0] != b'/' && go(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && go(&p[1..], &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

/// The types whose methods the language provides itself, as the checker names them in a probe program, and
/// the name they are documented under.
const BUILTIN_TYPES: [(&str, &str); 10] = [
    ("string", "string"),
    ("int", "int"),
    ("float", "float"),
    ("array<T>", "array"),
    ("map<K, V>", "map"),
    ("Set<T>", "Set"),
    ("uuid", "uuid"),
    ("ulid", "ulid"),
    ("datetime", "datetime"),
    ("bytes", "bytes"),
];

const PROBE: &str = "fun probe<T, K, V>(a: array<T>, m: map<K, V>, s: string, i: int, f: float, u: uuid, l: ulid, d: datetime, b: bytes, set: Set<T>)\n  a\n  m\n  s\n  i\n  f\n  u\n  l\n  d\n  b\n  set\nend\n";

/// `qlang doc --prelude`: everything the language provides without an import, as JSON for the reference of the
/// site: the global functions, the methods of the built-in types, the `Type.function` names, and the types of the
/// prelude with their fields and methods. The explanations are the ones editors show.
fn prelude_reference() -> Result<String, String> {
    use qlang_core::check::catalog;
    let mut host = crate::new_host_with(
        Path::new("probe.q"),
        None,
        Some(0),
        Vec::new(),
        &[],
        true,
        false,
    )?;
    let program = compile_with("probe.q", PROBE, &mut host, &CompileOptions { info: true })
        .map_err(|e| format!("the probe does not compile:\n{}", e.render()))?;
    let info = program.info.ok_or("no information from the compiler")?;
    let entry = |name: &str, signature: &str, doc: &str| serde_json::json!({"name": name, "signature": signature, "doc": doc});
    let functions: Vec<_> = info
        .functions
        .iter()
        .map(|f| entry(&f.name, &f.detail, f.doc.as_deref().unwrap_or("")))
        .collect();
    let builtin: Vec<_> = BUILTIN_TYPES
        .iter()
        .map(|(key, name)| {
            let methods: Vec<_> = info
                .members
                .get(*key)
                .map(|ms| {
                    ms.iter()
                        .filter(|m| m.kind == "method")
                        .map(|m| entry(&m.name, &m.detail, m.doc.as_deref().unwrap_or("")))
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({"name": name, "methods": methods})
        })
        .collect();
    let mut owners: Vec<&str> = Vec::new();
    for (owner, ..) in catalog::STATICS {
        if !owners.contains(owner) {
            owners.push(owner);
        }
    }
    let namespaces: Vec<_> = owners
        .iter()
        .map(|o| {
            let list: Vec<_> = catalog::STATICS
                .iter()
                .filter(|(owner, ..)| owner == o)
                .map(|(_, kind, name, detail, doc)| {
                    serde_json::json!({"kind": kind, "name": name, "signature": detail, "doc": doc})
                })
                .collect();
            serde_json::json!({"name": o, "entries": list})
        })
        .collect();
    let items: Vec<&ItemInfo> = info
        .items
        .iter()
        .filter(|it| it.loc.file == "<prelude>")
        .collect();
    let types: Vec<_> = items
        .iter()
        .filter(|it| it.owner.is_none() && matches!(it.kind, "struct" | "enum" | "trait"))
        .map(|top| {
            let of = |kind: &str| -> Vec<serde_json::Value> {
                items
                    .iter()
                    .filter(|m| {
                        // a generic impl names its owner `Result<T, E>`
                        m.kind == kind
                            && m.owner.as_deref().map(|o| o.split('<').next().unwrap_or(o))
                                == Some(top.name.as_str())
                    })
                    .map(|m| serde_json::json!({"name": m.name, "signature": m.label, "doc": m.doc.clone().unwrap_or_default()}))
                    .collect()
            };
            serde_json::json!({
                "name": top.name,
                "kind": top.kind,
                "doc": top.doc.clone().unwrap_or_default(),
                "fields": of("field"),
                "variants": of("variant"),
                "methods": of("method"),
            })
        })
        .collect();
    let all = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "functions": functions,
        "builtin": builtin,
        "namespaces": namespaces,
        "types": types,
    });
    serde_json::to_string_pretty(&all).map_err(|e| e.to_string())
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut out_file: Option<PathBuf> = None;
    let mut require_levels = false;
    let mut prelude = false;
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
            "--require-levels" => require_levels = true,
            "--prelude" => prelude = true,
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
        i += 1;
    }
    if prelude {
        return match prelude_reference() {
            Ok(text) => {
                match &out_file {
                    Some(f) => {
                        if let Err(e) = std::fs::write(f, text + "\n") {
                            eprintln!("cannot write {}: {e}", f.display());
                            return ExitCode::from(1);
                        }
                    }
                    None => println!("{text}"),
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        };
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
    let levels = Levels::load(&crate::project_dir(&cwd).unwrap_or(cwd.clone()));
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
        let level = levels.of(&shown);
        if require_levels && level.is_none() {
            eprintln!(
                "{shown}: no stability level (add it to `[stability]` in qlang.toml: stable, provisional or experimental)"
            );
            failed = true;
        }
        match document(file, &shown, level) {
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
    eprintln!(
        "{why}\nusage: qlang doc [path...] [-o FILE] [--require-levels] | qlang doc --prelude [-o FILE]"
    );
    ExitCode::from(64)
}

/// The Markdown section of one file.
fn document(file: &Path, shown: &str, level: Option<&str>) -> Result<String, String> {
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
    Ok(render(shown, level, &items))
}

/// A documentation comment split at its tags: `@param name text` and `@return text` (also
/// `@returns`). A line that starts with neither continues the tag above it.
#[derive(Default)]
struct Doc {
    summary: String,
    params: Vec<(String, String)>,
    returns: Option<String>,
}

fn split_doc(doc: &str) -> Doc {
    #[derive(PartialEq)]
    enum Into {
        Summary,
        Param,
        Return,
    }
    let mut out = Doc::default();
    let mut into = Into::Summary;
    for line in doc.trim().lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("@param") {
            let rest = rest.trim_start();
            let (name, text) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            out.params.push((name.to_string(), text.trim().to_string()));
            into = Into::Param;
        } else if let Some(rest) = line
            .strip_prefix("@returns")
            .or_else(|| line.strip_prefix("@return"))
        {
            out.returns = Some(rest.trim().to_string());
            into = Into::Return;
        } else {
            let target = match into {
                Into::Summary => Some(&mut out.summary),
                Into::Param => out.params.last_mut().map(|p| &mut p.1),
                Into::Return => out.returns.as_mut(),
            };
            if let Some(t) = target {
                if !t.is_empty() && !line.is_empty() {
                    t.push(' ');
                }
                t.push_str(line);
            }
        }
    }
    out.summary = out.summary.trim().to_string();
    out
}

/// The tags of a documentation comment, as a Markdown list under `indent`.
fn render_tags(doc: &Doc, indent: &str, out: &mut String) {
    for (name, text) in &doc.params {
        out.push_str(&format!("{indent}- parameter `{name}`"));
        if !text.is_empty() {
            out.push_str(&format!(" — {text}"));
        }
        out.push('\n');
    }
    if let Some(text) = &doc.returns {
        out.push_str(&format!("{indent}- returns {text}\n"));
    }
}

fn is_top_level(it: &ItemInfo) -> bool {
    it.owner.is_none()
}

fn render(file: &str, level: Option<&str>, items: &[&ItemInfo]) -> String {
    let mut out = format!("# {file}\n\n");
    if let Some(level) = level {
        out.push_str(&format!("_Stability: **{level}**._\n\n"));
    }
    let tops: Vec<&&ItemInfo> = items.iter().filter(|it| is_top_level(it)).collect();
    if tops.is_empty() {
        out.push_str("_Nothing documented here._\n\n");
    }
    for top in tops {
        out.push_str(&format!("## `{}`\n\n", top.label));
        if let Some(doc) = &top.doc {
            let doc = split_doc(doc);
            if !doc.summary.is_empty() {
                out.push_str(&doc.summary);
                out.push_str("\n\n");
            }
            if !doc.params.is_empty() || doc.returns.is_some() {
                render_tags(&doc, "", &mut out);
                out.push('\n');
            }
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
                let doc = m.doc.as_deref().map(split_doc).unwrap_or_default();
                if !doc.summary.is_empty() {
                    out.push_str(" — ");
                    out.push_str(&doc.summary);
                }
                out.push('\n');
                render_tags(&doc, "  ", &mut out);
            }
            out.push('\n');
        }
    }
    out
}
