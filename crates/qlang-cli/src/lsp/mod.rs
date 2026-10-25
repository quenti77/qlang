//! `qlang lsp`: a language server (Language Server Protocol, over standard input and output).
//!
//! It offers errors as you type, hovers, completion, signature help, go to definition, find
//! references, rename and symbols. Every answer comes from compiling the file the editor
//! shows (unsaved text included) with the checker's information turned on.

mod intel;

use crate::packages::Packages;
use qlang_core::check::{CompileOptions, Info, compile_with};
use qlang_core::diag::Diagnostic;
use qlang_core::host::Host;
use qlang_core::span::Location;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

// ------------------------------------------------------------------- transport

fn read_messages(tx: mpsc::Sender<Value>) {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut length: Option<usize> = None;
        loop {
            let mut line = String::new();
            match input.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(v) = line
                .strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
            {
                length = v.trim().parse().ok();
            }
        }
        let Some(n) = length else {
            continue;
        };
        let mut body = vec![0u8; n];
        if input.read_exact(&mut body).is_err() {
            return;
        }
        if let Ok(v) = serde_json::from_slice::<Value>(&body)
            && tx.send(v).is_err()
        {
            return;
        }
    }
}

fn send(msg: &Value) {
    let body = msg.to_string();
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = out.flush();
}

fn respond(id: &Value, result: Value) {
    send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
}

fn respond_error(id: &Value, code: i64, message: &str) {
    send(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
}

fn notify(method: &str, params: Value) {
    send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
}

// ------------------------------------------------------------------------ URIs

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::new();
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%'
            && i + 2 < raw.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            bytes.push(b);
            i += 3;
            continue;
        }
        bytes.push(raw[i]);
        i += 1;
    }
    let text = String::from_utf8(bytes).ok()?;
    // `/C:/dir/file` on Windows
    let b = text.as_bytes();
    let text = if b.len() >= 3 && b[0] == b'/' && b[2] == b':' {
        text[1..].to_string()
    } else {
        text
    };
    Some(PathBuf::from(text))
}

fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:@!$&'()*+,;=".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ----------------------------------------------------------------------- host

/// The disk, except for the files the editor has open: their text wins. Imports are
/// relative to `root`.
struct LspHost {
    root: PathBuf,
    overlay: HashMap<PathBuf, String>,
    packages: Packages,
}

impl LspHost {
    fn key(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}

impl Host for LspHost {
    fn print(&mut self, _text: &str) {}

    fn load_module(&mut self, path: &str) -> Result<String, String> {
        let full = if qlang_core::span::is_package_path(path) {
            self.packages.resolve(path)?
        } else {
            self.root.join(path)
        };
        if full.extension().is_none_or(|e| e != "q") {
            return Err(format!(
                "\"{path}\" is not a qlang file (only .q files can be imported)"
            ));
        }
        if let Some(text) = self.overlay.get(&Self::key(&full)) {
            return Ok(text.clone());
        }
        std::fs::read_to_string(&full).map_err(|_| format!("file \"{path}\" not found"))
    }

    fn package_file(&mut self, path: &str) -> String {
        self.packages.with_main(path)
    }

    fn canonical_path(&mut self, path: &str) -> String {
        if qlang_core::span::is_package_path(path) {
            return match self.packages.resolve(path) {
                Ok(full) => full.to_string_lossy().into_owned(),
                Err(_) => path.to_string(),
            };
        }
        path.to_string()
    }

    // an editor cannot know what the real host provides
    fn has_extern(&self, _name: &str) -> bool {
        true
    }
}

// ---------------------------------------------------------------------- server

struct Doc {
    text: String,
    version: i64,
}

/// What the checker learned about one file, taken as the entry of a program.
struct Analysis {
    version: i64,
    root: PathBuf,
    entry: String,
    info: Option<Info>,
    diagnostics: Vec<Diagnostic>,
}

type Lines = Rc<Vec<Vec<char>>>;

struct Server {
    root: Option<PathBuf>,
    docs: HashMap<String, Doc>,
    analyses: HashMap<String, Analysis>,
    dirty: BTreeSet<String>,
    /// For each file with diagnostics: what each entry that reports it says.
    reported: HashMap<String, HashMap<String, Vec<Value>>>,
    lines: HashMap<PathBuf, Lines>,
    shutdown: bool,
}

/// A position inside a file, in characters (1-based, like the compiler's locations).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Loc {
    file: String,
    line: u32,
    col: u32,
    end_line: u32,
    end_col: u32,
}

impl From<&Location> for Loc {
    fn from(l: &Location) -> Loc {
        Loc {
            file: l.file.clone(),
            line: l.line,
            col: l.col,
            end_line: l.end_line,
            end_col: l.end_col,
        }
    }
}

impl Loc {
    fn start(&self) -> (String, u32, u32) {
        (self.file.clone(), self.line, self.col)
    }

    fn contains(&self, line1: u32, col1: u32) -> bool {
        if line1 < self.line || line1 > self.end_line {
            return false;
        }
        if line1 == self.line && col1 < self.col {
            return false;
        }
        // a cursor right after the name still belongs to it
        if line1 == self.end_line && col1 > self.end_col {
            return false;
        }
        true
    }
}

fn utf16_col(line: &[char], chars: usize) -> u32 {
    line.iter()
        .take(chars)
        .map(|c| c.len_utf16() as u32)
        .sum::<u32>()
        + chars.saturating_sub(line.len()) as u32
}

fn chars_from_utf16(line: &[char], units: u32) -> usize {
    let mut n = 0;
    let mut chars = 0;
    for c in line {
        if n >= units {
            break;
        }
        n += c.len_utf16() as u32;
        chars += 1;
    }
    chars
}

const KEYWORD_KINDS: &[(&str, u32)] = &[
    ("function", 3),
    ("method", 2),
    ("field", 5),
    ("variable", 6),
    ("param", 6),
    ("keyword", 14),
    ("type", 25),
    ("struct", 22),
    ("enum", 13),
    ("variant", 20),
    ("constant", 21),
    ("trait", 8),
    ("module", 9),
];

fn completion_kind(kind: &str) -> u32 {
    KEYWORD_KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(1, |(_, n)| *n)
}

fn symbol_kind(kind: &str) -> u32 {
    match kind {
        "function" => 12,
        "method" => 6,
        "struct" => 23,
        "enum" => 10,
        "trait" => 11,
        "field" => 8,
        "variant" => 22,
        "constant" => 14,
        "module" => 2,
        _ => 13,
    }
}

impl Server {
    fn new() -> Server {
        Server {
            root: None,
            docs: HashMap::new(),
            analyses: HashMap::new(),
            dirty: BTreeSet::new(),
            reported: HashMap::new(),
            lines: HashMap::new(),
            shutdown: false,
        }
    }

    // ------------------------------------------------------------- the project

    /// The directory imports are relative to for the file at `path`, and the file's name
    /// relative to it.
    ///
    /// The project is the folder of the nearest `qlang.toml` above the file (like the command
    /// line does), so a project that lives in a subfolder of what the editor opened still finds
    /// its packages. Without one, it is the folder the editor opened.
    fn project_of(&self, path: &Path) -> (PathBuf, String) {
        if let Some(config) = crate::providers::find_config(path)
            && let Some(dir) = config.parent()
            && let Ok(real) = path.canonicalize()
            && let Ok(rel) = real.strip_prefix(dir)
        {
            return (dir.to_path_buf(), posix(rel));
        }
        if let Some(root) = &self.root
            && let Ok(rel) = path.strip_prefix(root)
        {
            return (root.clone(), posix(rel));
        }
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (dir, name)
    }

    fn packages_of(root: &Path) -> Packages {
        Packages::load(root).unwrap_or_default()
    }

    fn overlay(&self) -> HashMap<PathBuf, String> {
        self.docs
            .iter()
            .filter_map(|(uri, d)| Some((LspHost::key(&uri_to_path(uri)?), d.text.clone())))
            .collect()
    }

    /// The real file behind a name the compiler reported.
    fn path_of(root: &Path, packages: &Packages, file: &str) -> PathBuf {
        if file.starts_with('@') {
            packages
                .resolve(file)
                .unwrap_or_else(|_| root.join(file.trim_start_matches('@')))
        } else {
            root.join(file)
        }
    }

    /// The lines of a file (the editor's text when it is open).
    fn lines_of(&mut self, path: &Path) -> Lines {
        let key = LspHost::key(path);
        if let Some(l) = self.lines.get(&key) {
            return l.clone();
        }
        let text = self
            .docs
            .iter()
            .find(|(uri, _)| uri_to_path(uri).is_some_and(|p| LspHost::key(&p) == key))
            .map(|(_, d)| d.text.clone())
            .or_else(|| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let lines: Lines = Rc::new(
            text.split('\n')
                .map(|l| l.trim_end_matches('\r').chars().collect())
                .collect(),
        );
        self.lines.insert(key, lines.clone());
        lines
    }

    fn analyse(&mut self, uri: &str, text: Option<&str>) -> Option<Analysis> {
        let path = uri_to_path(uri)?;
        let (root, entry) = self.project_of(&path);
        let source = match text {
            Some(t) => t.to_string(),
            None => match self.docs.get(uri) {
                Some(d) => d.text.clone(),
                // not open in the editor: the disk
                None => std::fs::read_to_string(&path).ok()?,
            },
        };
        let version = self.docs.get(uri).map_or(0, |d| d.version);
        let mut overlay = self.overlay();
        overlay.insert(LspHost::key(&path), source.clone());
        let mut host = LspHost {
            packages: Self::packages_of(&root),
            root: root.clone(),
            overlay,
        };
        let options = CompileOptions { info: true };
        let (info, diagnostics) = match compile_with(&entry, &source, &mut host, &options) {
            Ok(p) => (p.info, p.warnings),
            Err(e) => (e.info.map(|i| *i), e.diagnostics),
        };
        Some(Analysis {
            version,
            root,
            entry,
            info,
            diagnostics,
        })
    }

    /// Like [`Self::analyse`], for code being typed: when it does not even parse (a block is still
    /// open), `end` is added at the bottom until it does, so that completion keeps working.
    fn analyse_unfinished(&mut self, uri: &str, text: &str) -> Option<Analysis> {
        let mut source = text.to_string();
        let mut first = None;
        for _ in 0..=6 {
            let a = self.analyse(uri, Some(&source))?;
            if a.info.is_some() {
                return Some(a);
            }
            first.get_or_insert(a);
            source.push_str("\nend");
        }
        first
    }

    /// The analysis of the open document, redone when the text changed since.
    fn analysis_for(&mut self, uri: &str) -> bool {
        if let Some(doc) = self.docs.get(uri)
            && let Some(a) = self.analyses.get(uri)
            && a.version == doc.version
            && a.info.is_some()
        {
            return true;
        }
        match self.analyse(uri, None) {
            Some(a) => {
                // the code may not compile far enough: the previous answer is better than none
                if a.info.is_none() && self.analyses.contains_key(uri) {
                    return true;
                }
                self.analyses.insert(uri.to_string(), a);
                true
            }
            None => false,
        }
    }

    // ------------------------------------------------------------- diagnostics

    fn range(lines: &[Vec<char>], l: &Loc) -> Value {
        let at = |line1: u32, col1: u32| {
            let li = (line1.max(1) - 1) as usize;
            let empty = Vec::new();
            let text = lines.get(li).unwrap_or(&empty);
            json!({ "line": li, "character": utf16_col(text, (col1.max(1) - 1) as usize) })
        };
        json!({ "start": at(l.line, l.col), "end": at(l.end_line, l.end_col) })
    }

    fn diagnostics_by_file(&mut self, entry_uri: &str) -> BTreeMap<String, Vec<Value>> {
        let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let Some(a) = self.analyses.get(entry_uri) else {
            return out;
        };
        let (root, entry) = (a.root.clone(), a.entry.clone());
        let diagnostics: Vec<Diagnostic> = a.diagnostics.clone();
        let packages = Self::packages_of(&root);
        for d in &diagnostics {
            let (file, mut loc) = match &d.location {
                Some(l) => (
                    l.file.clone(),
                    Loc {
                        file: l.file.clone(),
                        line: l.line,
                        col: l.col,
                        end_line: l.end_line,
                        end_col: l.end_col,
                    },
                ),
                None => (
                    entry.clone(),
                    Loc {
                        file: entry.clone(),
                        line: 1,
                        col: 1,
                        end_line: 1,
                        end_col: 2,
                    },
                ),
            };
            // never an empty range: it would be invisible
            if loc.end_line < loc.line || (loc.end_line == loc.line && loc.end_col <= loc.col) {
                loc.end_line = loc.line;
                loc.end_col = loc.col + 1;
            }
            let path = Self::path_of(&root, &packages, &file);
            let lines = self.lines_of(&path);
            let mut message = d.message.clone();
            let mut related = Vec::new();
            for n in &d.notes {
                match &n.location {
                    Some(nl) => {
                        let npath = Self::path_of(&root, &packages, &nl.file);
                        let nlines = self.lines_of(&npath);
                        let nloc = Loc {
                            file: nl.file.clone(),
                            line: nl.line,
                            col: nl.col,
                            end_line: nl.end_line.max(nl.line),
                            end_col: nl.end_col.max(nl.col + 1),
                        };
                        related.push(json!({
                            "location": {
                                "uri": path_to_uri(&npath),
                                "range": Self::range(&nlines, &nloc),
                            },
                            "message": n.message,
                        }));
                    }
                    None => {
                        message.push('\n');
                        message.push_str(&n.message);
                    }
                }
            }
            let severity = match d.severity {
                qlang_core::diag::Severity::Error => 1,
                qlang_core::diag::Severity::Warning => 2,
            };
            let mut item = json!({
                "range": Self::range(&lines, &loc),
                "severity": severity,
                "code": d.code,
                "source": "qlang",
                "message": message,
            });
            if !related.is_empty() {
                item["relatedInformation"] = Value::Array(related);
            }
            out.entry(path_to_uri(&path)).or_default().push(item);
        }
        out
    }

    /// Publish what the entry `entry_uri` reports (and clear what it no longer reports).
    fn publish(&mut self, entry_uri: &str) {
        let now = self.diagnostics_by_file(entry_uri);
        let mut touched: BTreeSet<String> = now.keys().cloned().collect();
        for (target, per_entry) in self.reported.iter_mut() {
            if per_entry.remove(entry_uri).is_some() {
                touched.insert(target.clone());
            }
        }
        for (target, list) in now {
            self.reported
                .entry(target)
                .or_default()
                .insert(entry_uri.to_string(), list);
        }
        // the file itself always gets an answer, even an empty one
        touched.insert(entry_uri.to_string());
        for target in touched {
            let mut merged: Vec<Value> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            if let Some(per_entry) = self.reported.get(&target) {
                for list in per_entry.values() {
                    for d in list {
                        if seen.insert(d.to_string()) {
                            merged.push(d.clone());
                        }
                    }
                }
            }
            notify(
                "textDocument/publishDiagnostics",
                json!({ "uri": target, "diagnostics": merged }),
            );
        }
        self.reported.retain(|_, per_entry| !per_entry.is_empty());
    }

    fn refresh(&mut self, uri: &str) {
        if let Some(a) = self.analyse(uri, None) {
            self.analyses.insert(uri.to_string(), a);
            self.publish(uri);
        }
    }

    fn flush_dirty(&mut self) {
        let dirty: Vec<String> = std::mem::take(&mut self.dirty).into_iter().collect();
        for uri in dirty {
            if self.docs.contains_key(&uri) {
                self.lines.clear();
                self.refresh(&uri);
            }
        }
    }

    // ------------------------------------------------------------- requests

    fn text_position(&mut self, params: &Value) -> Option<(String, usize, usize)> {
        let uri = params["textDocument"]["uri"].as_str()?.to_string();
        let line0 = params["position"]["line"].as_u64()? as usize;
        let units = params["position"]["character"].as_u64()? as u32;
        let path = uri_to_path(&uri)?;
        let lines = self.lines_of(&path);
        let text = lines.get(line0)?;
        Some((uri, line0, chars_from_utf16(text, units)))
    }

    fn hover(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        if !self.analysis_for(&uri) {
            return Value::Null;
        }
        let Some(path) = uri_to_path(&uri) else {
            return Value::Null;
        };
        let lines = self.lines_of(&path);
        let Some(a) = self.analyses.get(&uri) else {
            return Value::Null;
        };
        let Some(info) = &a.info else {
            return Value::Null;
        };
        let Some(h) = intel::hover_at(info, &a.entry, line0, col, &lines[line0]) else {
            return Value::Null;
        };
        let text = &lines[line0];
        json!({
            "contents": { "kind": "markdown", "value": h.markdown },
            "range": {
                "start": { "line": line0, "character": utf16_col(text, h.start) },
                "end": { "line": line0, "character": utf16_col(text, h.end) },
            },
        })
    }

    fn item_json(item: &intel::Item, range: Option<&Value>) -> Value {
        let mut v = json!({
            "label": item.label,
            "kind": completion_kind(&item.kind),
            "detail": item.detail,
            "sortText": item.sort,
        });
        if let Some(doc) = &item.doc {
            v["documentation"] = json!({ "kind": "markdown", "value": doc });
        }
        // a function or a method is written with its parentheses, the cursor between them
        let call = matches!(item.kind.as_str(), "function" | "method");
        let text = if call {
            v["insertTextFormat"] = json!(2);
            intel::call_snippet(&item.label, &item.detail)
        } else {
            item.label.clone()
        };
        if let Some(r) = range {
            v["textEdit"] = json!({ "range": r, "newText": text });
        } else if call {
            v["insertText"] = json!(text);
        }
        v
    }

    /// Inside the string of an `import`: the packages (`@std/`), the files and folders of a
    /// package, or those next to the file. `None` anywhere else.
    fn import_completions(
        &self,
        path: &Path,
        line: &[char],
        line0: usize,
        col: usize,
    ) -> Option<Value> {
        let before: String = line[..col.min(line.len())].iter().collect();
        if !before.trim_start().starts_with("import") {
            return None;
        }
        // the cursor is in a string when the quotes before it are odd in number
        if before.matches('"').count() & 1 == 0 {
            return None;
        }
        let quote = before.rfind('"')?;
        let typed = &before[quote + 1..];
        let (project, _) = self.project_of(path);
        let packages = Self::packages_of(&project);
        // what is replaced: the part of the path after the last slash
        let segment_start = typed.rfind('/').map_or(0, |i| i + 1);
        let segment_chars = typed[segment_start..].chars().count();
        let range = json!({
            "start": { "line": line0, "character": utf16_col(line, col - segment_chars) },
            "end": { "line": line0, "character": utf16_col(line, col) },
        });
        let item = |label: &str, kind: u32, folder: bool, detail: &str| {
            let mut it = json!({
                "label": label,
                "kind": kind,
                "detail": detail,
                "textEdit": { "range": range, "newText": label },
                "sortText": format!("{}{label}", if folder { "0" } else { "1" }),
            });
            if folder {
                // a folder is not the end of the path: go on proposing
                it["command"] = json!({ "title": "", "command": "editor.action.triggerSuggest" });
            }
            it
        };
        let mut items: Vec<Value> = Vec::new();
        let listing = |dir: &Path, skip: Option<&str>| -> Vec<(String, bool)> {
            let mut out: Vec<(String, bool)> = Vec::new();
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') || ["target", "node_modules"].contains(&name.as_str())
                    {
                        continue;
                    }
                    let is_dir = e.path().is_dir();
                    if (is_dir || name.ends_with(".q")) && Some(name.as_str()) != skip {
                        out.push((name, is_dir));
                    }
                }
            }
            out.sort();
            out
        };
        if let Some(rest) = typed.strip_prefix('@') {
            match rest.split_once('/') {
                None => {
                    // the name of a package
                    for name in packages.names() {
                        items.push(item(&format!("@{name}/"), 9, true, "package"));
                    }
                    // the segment before the slash includes the arobase
                    let start = col - typed.chars().count();
                    let range = json!({
                        "start": { "line": line0, "character": utf16_col(line, start) },
                        "end": { "line": line0, "character": utf16_col(line, col) },
                    });
                    for it in &mut items {
                        it["textEdit"]["range"] = range.clone();
                    }
                }
                Some((name, inside)) => {
                    let base = packages.dir_of(name)?;
                    let sub = inside.rfind('/').map_or("", |i| &inside[..i]);
                    if sub.split('/').any(|p| p == "..") {
                        return Some(Value::Array(Vec::new()));
                    }
                    for (n, is_dir) in listing(&base.join(sub), None) {
                        if is_dir {
                            items.push(item(&format!("{n}/"), 19, true, "folder"));
                        } else {
                            items.push(item(&n, 17, false, "file"));
                        }
                    }
                }
            }
        } else {
            let dir_part = typed.rfind('/').map_or("", |i| &typed[..=i]);
            let here = path.parent().unwrap_or(Path::new("."));
            let own = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if dir_part.is_empty() {
                items.push(item("./", 19, true, "this folder"));
                items.push(item("../", 19, true, "the folder above"));
                for name in packages.names() {
                    let label = format!("@{name}/");
                    let mut it = item(&label, 9, true, "package");
                    // the arobase is part of what is typed
                    it["textEdit"]["range"] = json!({
                        "start": { "line": line0, "character": utf16_col(line, col - typed.chars().count()) },
                        "end": { "line": line0, "character": utf16_col(line, col) },
                    });
                    items.push(it);
                }
            } else {
                for (n, is_dir) in listing(&here.join(dir_part), own.as_deref()) {
                    if is_dir {
                        items.push(item(&format!("{n}/"), 19, true, "folder"));
                    } else {
                        items.push(item(&n, 17, false, "file"));
                    }
                }
                if dir_part == "./" || dir_part.ends_with("../") {
                    items.push(item("../", 19, true, "the folder above"));
                }
            }
        }
        Some(Value::Array(items))
    }

    fn completion(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        let Some(path) = uri_to_path(&uri) else {
            return Value::Null;
        };
        let lines = self.lines_of(&path);
        let line = lines[line0].clone();
        // the quotes, the slash and the arobase only start a completion inside an `import`
        let trigger = params["context"]["triggerCharacter"].as_str();
        if let Some(items) = self.import_completions(&path, &line, line0, col) {
            return items;
        }
        if matches!(trigger, Some("\"" | "/" | "@")) {
            return Value::Null;
        }
        let line_range = |from: usize, to: usize| {
            json!({
                "start": { "line": line0, "character": utf16_col(&line, from) },
                "end": { "line": line0, "character": utf16_col(&line, to) },
            })
        };
        if let Some((dot, start)) = intel::member_access_at(&line, col) {
            // `value.par|`: ask the compiler about the code without `.par`, so that it compiles
            let Some(doc) = self.docs.get(&uri) else {
                return Value::Null;
            };
            // after `->` or `:` a type must stay there, or the line does not even parse
            let in_type = intel::type_position(&line, dot);
            let blanked = match in_type {
                Some(from) => intel::blank_as_type(&doc.text, line0, from, col),
                None => intel::blank(&doc.text, line0, dot, col),
            };
            let Some(a) = self.analyse_unfinished(&uri, &blanked) else {
                return Value::Null;
            };
            let Some(info) = &a.info else {
                return Value::Null;
            };
            let before: String = line[..dot].iter().collect();
            let range = line_range(start, col);
            let mut items = intel::member_completions(info, &a.entry, line0, dot, &before);
            if items.is_empty() {
                items = intel::alias_completions(info, &a.entry, &before);
            }
            return Value::Array(
                items
                    .iter()
                    .map(|i| Self::item_json(i, Some(&range)))
                    .collect(),
            );
        }
        // an unfinished word is only an unknown name to the compiler; open blocks are closed
        let typed_text = match self.docs.get(&uri) {
            Some(doc) => doc.text.clone(),
            None => return Value::Null,
        };
        let current = self.analyses.get(&uri).is_some_and(|a| {
            a.info.is_some() && self.docs.get(&uri).is_some_and(|d| d.version == a.version)
        });
        let rebuilt;
        let a = if current {
            &self.analyses[&uri]
        } else {
            let Some(fresh) = self.analyse_unfinished(&uri, &typed_text) else {
                return Value::Null;
            };
            if fresh.info.is_some() {
                rebuilt = fresh;
                &rebuilt
            } else if !self.analysis_for(&uri) {
                return Value::Null;
            } else {
                &self.analyses[&uri]
            }
        };
        let Some(info) = &a.info else {
            return Value::Null;
        };
        let all: Vec<char> = self.docs[&uri].text.chars().collect();
        let offset: usize = lines.iter().take(line0).map(|l| l.len() + 1).sum::<usize>() + col;
        if let Some(lit) = intel::struct_lit_at(&all, offset) {
            let items = intel::struct_field_completions(info, &lit);
            if !items.is_empty() {
                let range = line_range(col - lit.prefix.chars().count(), col);
                return Value::Array(
                    items
                        .iter()
                        .map(|i| Self::item_json(i, Some(&range)))
                        .collect(),
                );
            }
        }
        Value::Array(
            intel::name_completions(info, &a.entry, line0, col)
                .iter()
                .map(|i| Self::item_json(i, None))
                .collect(),
        )
    }

    fn signature_help(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        let Some(path) = uri_to_path(&uri) else {
            return Value::Null;
        };
        let lines = self.lines_of(&path);
        let line = &lines[line0];
        let Some(call) = intel::call_at(line, col) else {
            return Value::Null;
        };
        let Some(doc) = self.docs.get(&uri) else {
            return Value::Null;
        };
        let blanked = intel::blank(&doc.text, line0, call.open, col);
        let Some(a) = self.analyse(&uri, Some(&blanked)) else {
            return Value::Null;
        };
        let Some(info) = &a.info else {
            return Value::Null;
        };
        let Some(sig) = intel::signature_for(info, &a.entry, line0, line, &call) else {
            return Value::Null;
        };
        let mut s = json!({
            "label": sig.label,
            "parameters": sig.parameters.iter().map(|p| json!({ "label": p })).collect::<Vec<_>>(),
        });
        if let Some(d) = &sig.doc {
            s["documentation"] = json!({ "kind": "markdown", "value": d });
        }
        json!({
            "signatures": [s],
            "activeSignature": 0,
            "activeParameter": sig.active.min(sig.parameters.len().saturating_sub(1)),
        })
    }

    // ----------------------------------------------------------- navigation

    /// The definition the name under the cursor stands for (the name itself when the
    /// cursor is on a definition).
    fn definition_at(&mut self, uri: &str, line0: usize, col: usize) -> Option<(Loc, Loc)> {
        if !self.analysis_for(uri) {
            return None;
        }
        let a = self.analyses.get(uri)?;
        let info = a.info.as_ref()?;
        let (line1, col1) = (line0 as u32 + 1, col as u32 + 1);
        let file = &a.entry;
        let used = info
            .refs
            .iter()
            .find(|r| r.loc.file == *file && Loc::from(&r.loc).contains(line1, col1));
        if let Some(r) = used {
            return Some((Loc::from(&r.loc), Loc::from(&r.def)));
        }
        for l in &info.locals {
            let loc = Loc::from(&l.loc);
            if loc.file == *file && loc.contains(line1, col1) {
                return Some((loc.clone(), loc));
            }
        }
        for it in &info.items {
            let loc = Loc::from(&it.loc);
            if loc.file == *file && loc.contains(line1, col1) {
                return Some((loc.clone(), loc));
            }
        }
        None
    }

    fn location_json(&mut self, root: &Path, packages: &Packages, l: &Loc) -> Value {
        let path = Self::path_of(root, packages, &l.file);
        let lines = self.lines_of(&path);
        json!({ "uri": path_to_uri(&path), "range": Self::range(&lines, l) })
    }

    fn definition(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        let Some((_, def)) = self.definition_at(&uri, line0, col) else {
            return Value::Null;
        };
        let root = self.analyses[&uri].root.clone();
        let packages = Self::packages_of(&root);
        self.location_json(&root, &packages, &def)
    }

    /// Every `.q` file of the project (the open documents included).
    fn project_files(&self, root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "q") {
                    out.push(p);
                }
                if out.len() >= 400 {
                    return out;
                }
            }
        }
        out.sort();
        out
    }

    /// All the analyses of the project: each file taken as an entry.
    fn project_analyses(&mut self, root: &Path) -> Vec<Analysis> {
        let mut out = Vec::new();
        let overlay = self.overlay();
        for path in self.project_files(root) {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let entry = posix(rel);
            let source = match overlay.get(&LspHost::key(&path)) {
                Some(t) => t.clone(),
                None => match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(_) => continue,
                },
            };
            let mut host = LspHost {
                root: root.to_path_buf(),
                overlay: overlay.clone(),
                packages: Self::packages_of(root),
            };
            let options = CompileOptions { info: true };
            let (info, diagnostics) = match compile_with(&entry, &source, &mut host, &options) {
                Ok(p) => (p.info, p.warnings),
                Err(e) => (e.info.map(|i| *i), e.diagnostics),
            };
            out.push(Analysis {
                version: 0,
                root: root.to_path_buf(),
                entry,
                info,
                diagnostics,
            });
        }
        out
    }

    /// Where the definition `target` is used, in the whole project (the definition included).
    fn usages(&mut self, root: &Path, target: &Loc) -> (BTreeSet<Loc>, Vec<RefInfoOwned>) {
        let mut found: BTreeSet<Loc> = BTreeSet::new();
        let mut all_refs: Vec<RefInfoOwned> = Vec::new();
        found.insert(target.clone());
        for a in self.project_analyses(root) {
            let Some(info) = &a.info else {
                continue;
            };
            for r in &info.refs {
                let def = Loc::from(&r.def);
                let at = Loc::from(&r.loc);
                if def.start() == target.start() {
                    found.insert(at.clone());
                }
                all_refs.push(RefInfoOwned { at, def });
            }
        }
        (found, all_refs)
    }

    fn references(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        let Some((_, def)) = self.definition_at(&uri, line0, col) else {
            return json!([]);
        };
        let root = self.analyses[&uri].root.clone();
        let packages = Self::packages_of(&root);
        let with_decl = params["context"]["includeDeclaration"]
            .as_bool()
            .unwrap_or(true);
        let (found, _) = self.usages(&root, &def);
        let mut out = Vec::new();
        for l in found {
            if !with_decl && l.start() == def.start() {
                continue;
            }
            out.push(self.location_json(&root, &packages, &l));
        }
        Value::Array(out)
    }

    fn word_range(&mut self, uri: &str, line0: usize, col: usize) -> Option<(String, Value)> {
        let path = uri_to_path(uri)?;
        let lines = self.lines_of(&path);
        let text = lines.get(line0)?;
        let (word, s, e) = intel::word_at(text, col)?;
        Some((
            word,
            json!({
                "start": { "line": line0, "character": utf16_col(text, s) },
                "end": { "line": line0, "character": utf16_col(text, e) },
            }),
        ))
    }

    fn prepare_rename(&mut self, params: &Value) -> Value {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return Value::Null;
        };
        let Some((_, def)) = self.definition_at(&uri, line0, col) else {
            return Value::Null;
        };
        if def.file.starts_with('@') {
            return Value::Null;
        }
        match self.word_range(&uri, line0, col) {
            Some((word, range)) => json!({ "range": range, "placeholder": word }),
            None => Value::Null,
        }
    }

    fn rename(&mut self, id: &Value, params: &Value) {
        let Some((uri, line0, col)) = self.text_position(params) else {
            return respond(id, Value::Null);
        };
        let new_name = params["newName"].as_str().unwrap_or("").to_string();
        let valid = !new_name.is_empty()
            && !new_name.chars().next().is_some_and(|c| c.is_ascii_digit())
            && new_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && intel::keyword_doc(&new_name).is_none();
        if !valid {
            return respond_error(id, -32602, &format!("`{new_name}` is not a valid name"));
        }
        let Some((_, def)) = self.definition_at(&uri, line0, col) else {
            return respond_error(id, -32602, "there is no name to rename here");
        };
        let root = self.analyses[&uri].root.clone();
        let packages = Self::packages_of(&root);
        if def.file.starts_with('@') || def.file.starts_with("<") {
            return respond_error(id, -32602, "this name is defined outside of the project");
        }
        let old = {
            let path = Self::path_of(&root, &packages, &def.file);
            let lines = self.lines_of(&path);
            let line = &lines[(def.line - 1) as usize];
            line[(def.col - 1) as usize..(def.end_col - 1) as usize]
                .iter()
                .collect::<String>()
        };
        let (found, all_refs) = self.usages(&root, &def);
        let mut edits: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for l in &found {
            if l.file.starts_with('@') {
                return respond_error(id, -32602, "this name is also used in a package");
            }
            if l.line != l.end_line {
                continue;
            }
            // the same text meaning two things (`Point { x }`): renaming would change one
            if all_refs
                .iter()
                .any(|r| r.at.start() == l.start() && r.def.start() != def.start())
            {
                return respond_error(
                    id,
                    -32602,
                    &format!(
                        "line {}: `{old}` is both a field and a variable here (shorthand): write it out in full first",
                        l.line
                    ),
                );
            }
            let path = Self::path_of(&root, &packages, &l.file);
            let lines = self.lines_of(&path);
            let Some(text) = lines.get((l.line - 1) as usize) else {
                continue;
            };
            let slice: String = text
                .get((l.col - 1) as usize..(l.end_col - 1) as usize)
                .map(|s| s.iter().collect())
                .unwrap_or_default();
            if slice != old {
                continue;
            }
            edits
                .entry(path_to_uri(&path))
                .or_default()
                .push(json!({ "range": Self::range(&lines, l), "newText": new_name }));
        }
        respond(id, json!({ "changes": edits }));
    }

    /// The whole document laid out (`qlang fmt`), as one edit; nothing when it is already so or
    /// when it does not lex.
    fn formatting(&mut self, params: &Value) -> Value {
        let Some(uri) = params["textDocument"]["uri"].as_str() else {
            return Value::Null;
        };
        let Some(doc) = self.docs.get(uri) else {
            return Value::Null;
        };
        match qlang_core::fmt::format(&doc.text) {
            Ok(out) if out != doc.text => {
                let lines = doc.text.split('\n').count() as u64;
                json!([{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": lines, "character": 0 },
                    },
                    "newText": out,
                }])
            }
            _ => json!([]),
        }
    }

    fn document_symbols(&mut self, params: &Value) -> Value {
        let Some(uri) = params["textDocument"]["uri"].as_str().map(str::to_string) else {
            return Value::Null;
        };
        if !self.analysis_for(&uri) {
            return json!([]);
        }
        let Some(path) = uri_to_path(&uri) else {
            return json!([]);
        };
        let lines = self.lines_of(&path);
        let a = &self.analyses[&uri];
        let Some(info) = &a.info else {
            return json!([]);
        };
        let mine: Vec<&qlang_core::check::ItemInfo> = info
            .items
            .iter()
            .filter(|i| i.loc.file == a.entry)
            .collect();
        let symbol = |it: &qlang_core::check::ItemInfo, children: Vec<Value>| {
            let range = Self::range(&lines, &Loc::from(&it.loc));
            json!({
                "name": it.name,
                "detail": it.label,
                "kind": symbol_kind(it.kind),
                "range": range,
                "selectionRange": range,
                "children": children,
            })
        };
        let mut out = Vec::new();
        for it in mine.iter().filter(|i| i.owner.is_none()) {
            let children: Vec<Value> = mine
                .iter()
                .filter(|c| c.owner.as_deref() == Some(it.name.as_str()))
                .map(|c| symbol(c, Vec::new()))
                .collect();
            out.push(symbol(it, children));
        }
        // members whose owner is not defined in this file
        for it in mine.iter().filter(|i| {
            i.owner
                .as_ref()
                .is_some_and(|o| !mine.iter().any(|m| m.owner.is_none() && &m.name == o))
        }) {
            out.push(symbol(it, Vec::new()));
        }
        Value::Array(out)
    }

    fn workspace_symbols(&mut self, params: &Value) -> Value {
        let query = params["query"].as_str().unwrap_or("").to_lowercase();
        let Some(root) = self.root.clone() else {
            return json!([]);
        };
        let packages = Self::packages_of(&root);
        let mut seen: BTreeSet<Loc> = BTreeSet::new();
        let mut out = Vec::new();
        for a in self.project_analyses(&root) {
            let Some(info) = &a.info else {
                continue;
            };
            for it in &info.items {
                let loc = Loc::from(&it.loc);
                if loc.file.starts_with('@')
                    || !it.name.to_lowercase().contains(&query)
                    || !seen.insert(loc.clone())
                {
                    continue;
                }
                let location = self.location_json(&root, &packages, &loc);
                let mut s = json!({
                    "name": it.name,
                    "kind": symbol_kind(it.kind),
                    "location": location,
                });
                if let Some(o) = &it.owner {
                    s["containerName"] = json!(o);
                }
                out.push(s);
            }
        }
        Value::Array(out)
    }

    // ------------------------------------------------------------- dispatch

    fn initialize(&mut self, params: &Value) -> Value {
        let folder = params["workspaceFolders"][0]["uri"]
            .as_str()
            .or_else(|| params["rootUri"].as_str())
            .and_then(uri_to_path);
        self.root = folder;
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": { "openClose": true, "change": 1, "save": { "includeText": false } },
                "hoverProvider": true,
                "completionProvider": { "triggerCharacters": [".", "{", ",", "\"", "/", "@"] },
                "signatureHelpProvider": { "triggerCharacters": ["(", ","] },
                "definitionProvider": true,
                "referencesProvider": true,
                "renameProvider": { "prepareProvider": true },
                "documentSymbolProvider": true,
                "documentFormattingProvider": true,
                "workspaceSymbolProvider": true,
            },
            "serverInfo": { "name": "qlang", "version": env!("CARGO_PKG_VERSION") },
        })
    }

    fn notification(&mut self, method: &str, params: &Value) {
        match method {
            "textDocument/didOpen" => {
                let d = &params["textDocument"];
                if let (Some(uri), Some(text)) = (d["uri"].as_str(), d["text"].as_str()) {
                    self.docs.insert(
                        uri.to_string(),
                        Doc {
                            text: text.to_string(),
                            version: d["version"].as_i64().unwrap_or(0),
                        },
                    );
                    self.lines.clear();
                    self.refresh(uri);
                }
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str();
                let text = params["contentChanges"]
                    .as_array()
                    .and_then(|c| c.last())
                    .and_then(|c| c["text"].as_str());
                if let (Some(uri), Some(text)) = (uri, text) {
                    self.docs.insert(
                        uri.to_string(),
                        Doc {
                            text: text.to_string(),
                            version: params["textDocument"]["version"].as_i64().unwrap_or(0),
                        },
                    );
                    self.lines.clear();
                    self.dirty.insert(uri.to_string());
                }
            }
            "textDocument/didSave" => {
                // other open files may import this one
                let open: Vec<String> = self.docs.keys().cloned().collect();
                self.lines.clear();
                for uri in open {
                    self.dirty.insert(uri);
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.docs.remove(uri);
                    self.analyses.remove(uri);
                    self.dirty.remove(uri);
                    self.lines.clear();
                    let mut touched = Vec::new();
                    for (target, per_entry) in self.reported.iter_mut() {
                        if per_entry.remove(uri).is_some() {
                            touched.push(target.clone());
                        }
                    }
                    touched.push(uri.to_string());
                    for t in touched {
                        let merged: Vec<Value> = self
                            .reported
                            .get(&t)
                            .map(|m| m.values().flatten().cloned().collect())
                            .unwrap_or_default();
                        notify(
                            "textDocument/publishDiagnostics",
                            json!({ "uri": t, "diagnostics": merged }),
                        );
                    }
                }
            }
            "exit" => {
                std::process::exit(if self.shutdown { 0 } else { 1 });
            }
            _ => {}
        }
    }

    fn request(&mut self, id: &Value, method: &str, params: &Value) {
        self.lines.clear();
        match method {
            "initialize" => respond(id, self.initialize(params)),
            "shutdown" => {
                self.shutdown = true;
                respond(id, Value::Null);
            }
            "textDocument/hover" => respond(id, self.hover(params)),
            "textDocument/completion" => respond(id, self.completion(params)),
            "textDocument/signatureHelp" => respond(id, self.signature_help(params)),
            "textDocument/definition" => respond(id, self.definition(params)),
            "textDocument/references" => respond(id, self.references(params)),
            "textDocument/prepareRename" => respond(id, self.prepare_rename(params)),
            "textDocument/rename" => self.rename(id, params),
            "textDocument/documentSymbol" => respond(id, self.document_symbols(params)),
            "textDocument/formatting" => respond(id, self.formatting(params)),
            "workspace/symbol" => respond(id, self.workspace_symbols(params)),
            _ => respond_error(id, -32601, &format!("method not supported: {method}")),
        }
    }
}

/// A use of a name and the definition it stands for.
struct RefInfoOwned {
    at: Loc,
    def: Loc,
}

pub fn run() -> ExitCode {
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || read_messages(tx));
    let mut server = Server::new();
    loop {
        // typing bursts: wait a little for the next message before compiling
        let msg = if server.dirty.is_empty() {
            rx.recv().ok()
        } else {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(m) => Some(m),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    server.flush_dirty();
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => None,
            }
        };
        let Some(msg) = msg else {
            break;
        };
        let method = msg["method"].as_str().unwrap_or("");
        let params = &msg["params"];
        if msg.get("id").is_some() && !method.is_empty() {
            server.request(&msg["id"], method, params);
        } else if !method.is_empty() {
            server.notification(method, params);
        }
    }
    ExitCode::SUCCESS
}
