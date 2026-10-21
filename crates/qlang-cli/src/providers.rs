//! Extern providers: programs that implement `extern fun` for the terminal host.
//!
//! A provider is any program that speaks JSON lines on its standard input and output. It
//! is started once and stays alive for the whole run:
//!
//! ```text
//! -> {"op":"list"}                                  <- {"externs":["mail_send"]}
//! -> {"op":"call","name":"mail_send","args":[...]}  <- {"ok":<value>}  or  {"error":"why"}
//! ```
//!
//! Providers are declared in a `qlang.toml` found next to the program (or in a parent
//! directory), and with `--extern "command"` on the command line:
//!
//! ```toml
//! [externs]
//! mail = "python3 mail.py"
//! ```
//!
//! Commands run from the directory of the `qlang.toml` (the current directory for
//! `--extern`), without a shell.

use qlang_core::host::{HostError, HostValue};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// The longest answer (one line) a provider may give.
const MAX_ANSWER: u64 = 64 << 20;

/// One provider to start.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Its name in `qlang.toml` (`cli` for `--extern`), used in messages.
    pub label: String,
    pub command: String,
    pub dir: PathBuf,
    /// The `qlang.toml` that declared it (`None` for `--extern`, which the user typed).
    pub config: Option<PathBuf>,
}

struct Provider {
    label: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Provider {
    fn start(spec: &Spec) -> Result<(Provider, Vec<String>), String> {
        let words = split_command(&spec.command)?;
        let (program, args) = words
            .split_first()
            .ok_or_else(|| format!("provider `{}`: the command is empty", spec.label))?;
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&spec.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                format!(
                    "provider `{}`: cannot start `{}`: {e}",
                    spec.label, spec.command
                )
            })?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = BufReader::new(child.stdout.take().expect("piped"));
        let mut p = Provider {
            label: spec.label.clone(),
            child,
            stdin,
            stdout,
        };
        let answer = p
            .request(&serde_json::json!({ "op": "list" }))
            .map_err(|e| format!("provider `{}`: {e}", spec.label))?;
        let names = answer
            .get("externs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                format!(
                    "provider `{}`: the answer to `list` must be {{\"externs\": [names]}}",
                    spec.label
                )
            })?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        Ok((p, names))
    }

    fn request(&mut self, message: &Json) -> Result<Json, String> {
        let mut line = serde_json::to_string(message).expect("a JSON value serializes");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|_| "the provider stopped".to_string())?;
        let mut answer = String::new();
        // a provider that never ends its line must not fill the memory
        match std::io::Read::take(&mut self.stdout, MAX_ANSWER).read_line(&mut answer) {
            Ok(0) | Err(_) => Err("the provider stopped".to_string()),
            Ok(_) if !answer.ends_with('\n') && answer.len() as u64 >= MAX_ANSWER => Err(format!(
                "the provider's answer is longer than {} MB",
                MAX_ANSWER >> 20
            )),
            Ok(_) => serde_json::from_str(&answer)
                .map_err(|e| format!("the provider answered something that is not JSON ({e})")),
        }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The running providers and the names each one implements.
#[derive(Default)]
pub struct Providers {
    running: Vec<Provider>,
    names: HashMap<String, usize>,
}

impl Providers {
    pub fn start(specs: &[Spec], reserved: &[&str]) -> Result<Providers, String> {
        let mut out = Providers::default();
        for spec in specs {
            let (provider, names) = Provider::start(spec)?;
            let index = out.running.len();
            for name in names {
                if reserved.contains(&name.as_str()) {
                    return Err(format!(
                        "provider `{}` offers `{name}`, which qlang already provides",
                        spec.label
                    ));
                }
                if let Some(&other) = out.names.get(&name) {
                    return Err(format!(
                        "`{name}` is offered by both `{}` and `{}`",
                        out.running[other].label, spec.label
                    ));
                }
                out.names.insert(name, index);
            }
            out.running.push(provider);
        }
        Ok(out)
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub fn call(&mut self, name: &str, args: Vec<HostValue>) -> Result<HostValue, HostError> {
        let Some(&index) = self.names.get(name) else {
            return Err(HostError::Unsupported("extern"));
        };
        let p = &mut self.running[index];
        let message = serde_json::json!({
            "op": "call",
            "name": name,
            "args": args.iter().map(to_json).collect::<Vec<_>>(),
        });
        let answer = p
            .request(&message)
            .map_err(|e| HostError::Other(format!("provider `{}`: {e}", p.label)))?;
        if let Some(why) = answer.get("error") {
            return Err(HostError::Other(match why {
                Json::String(s) => s.clone(),
                other => other.to_string(),
            }));
        }
        match answer.get("ok") {
            Some(v) => Ok(from_json(v)),
            None => Err(HostError::Other(format!(
                "provider `{}`: the answer has neither `ok` nor `error`",
                p.label
            ))),
        }
    }
}

/// Values go over the wire as JSON: structs and maps are objects (keys of other types
/// are written as text), enums are the name of their variant.
fn to_json(v: &HostValue) -> Json {
    match v {
        HostValue::Null => Json::Null,
        HostValue::Bool(b) => Json::Bool(*b),
        HostValue::Int(i) => Json::from(*i),
        HostValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number),
        HostValue::Str(s) => Json::String(s.clone()),
        HostValue::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        HostValue::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        HostValue::Str(s) => s.clone(),
                        HostValue::Int(i) => i.to_string(),
                        HostValue::Bool(b) => b.to_string(),
                        HostValue::Float(f) => f.to_string(),
                        other => to_json(other).to_string(),
                    };
                    (key, to_json(v))
                })
                .collect(),
        ),
    }
}

fn from_json(v: &Json) -> HostValue {
    match v {
        Json::Null => HostValue::Null,
        Json::Bool(b) => HostValue::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => HostValue::Int(i),
            None => HostValue::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => HostValue::Str(s.clone()),
        Json::Array(items) => HostValue::Array(items.iter().map(from_json).collect()),
        Json::Object(map) => HostValue::Map(
            map.iter()
                .map(|(k, v)| (HostValue::Str(k.clone()), from_json(v)))
                .collect(),
        ),
    }
}

/// Splits a command into words; single or double quotes group words (no shell, no
/// escapes: a backslash is an ordinary character so Windows paths work).
pub fn split_command(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(format!("unbalanced quote in `{text}`"));
    }
    if started {
        words.push(cur);
    }
    Ok(words)
}

/// The nearest `qlang.toml` from `start` (a file or a directory) upwards.
pub fn find_config(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir = if start.is_dir() {
        Some(start.as_path())
    } else {
        start.parent()
    };
    while let Some(d) = dir {
        let candidate = d.join("qlang.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// The providers declared by the `qlang.toml` near `program` and by `--extern` options.
pub fn specs_for(program: &Path, from_cli: &[String]) -> Result<Vec<Spec>, String> {
    let mut specs = Vec::new();
    if let Some(config) = find_config(program) {
        let text = std::fs::read_to_string(&config)
            .map_err(|e| format!("cannot read {}: {e}", config.display()))?;
        let table: toml::Table = text
            .parse()
            .map_err(|e| format!("{}: {e}", config.display()))?;
        let dir = config.parent().expect("a file has a parent").to_path_buf();
        if let Some(externs) = table.get("externs") {
            let externs = externs
                .as_table()
                .ok_or_else(|| format!("{}: `externs` must be a table", config.display()))?;
            for (label, command) in externs {
                let command = command.as_str().ok_or_else(|| {
                    format!(
                        "{}: externs.{label} must be a command (a string)",
                        config.display()
                    )
                })?;
                specs.push(Spec {
                    label: label.clone(),
                    command: command.to_string(),
                    dir: dir.clone(),
                    config: Some(config.clone()),
                });
            }
        }
    }
    let cwd =
        std::env::current_dir().map_err(|e| format!("cannot read the current directory: {e}"))?;
    for command in from_cli {
        specs.push(Spec {
            label: "cli".to_string(),
            command: command.clone(),
            dir: cwd.clone(),
            config: None,
        });
    }
    Ok(specs)
}

/// Where the commands the user agreed to run are remembered.
fn trust_file() -> Option<PathBuf> {
    let dir = std::env::var_os("QLANG_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|d| PathBuf::from(d).join("qlang")))
        .or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("qlang")))
        .or_else(|| {
            std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".config").join("qlang"))
        })?;
    Some(dir.join("trusted-externs"))
}

fn trust_line(spec: &Spec) -> Option<String> {
    let config = spec.config.as_ref()?;
    Some(format!(
        "{}\t{}\t{}",
        config.display(),
        spec.label,
        spec.command
    ))
}

/// A `qlang.toml` makes `qlang` run commands, so (like `direnv`) the first time it asks:
/// on a terminal it shows them and asks; otherwise `--trust-externs` must say yes. What
/// was agreed to is remembered per file, label and command; a changed command asks again.
/// Commands given with `--extern` are the user's own and need nothing.
pub fn ensure_trusted(specs: &[Spec], allow: bool) -> Result<(), String> {
    use std::io::IsTerminal;
    let known: std::collections::HashSet<String> = trust_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let unknown: Vec<&Spec> = specs
        .iter()
        .filter(|s| trust_line(s).is_some_and(|l| !known.contains(&l)))
        .collect();
    if unknown.is_empty() || allow {
        return Ok(());
    }
    let mut shown = String::new();
    for s in &unknown {
        let from = s
            .config
            .as_ref()
            .map(|c| c.display().to_string())
            .unwrap_or_default();
        shown.push_str(&format!("  {}: {}   (from {from})\n", s.label, s.command));
    }
    if !std::io::stdin().is_terminal() {
        return Err(format!(
            "qlang.toml wants to run these commands:\n{shown}pass --trust-externs to allow them for this run, or run `qlang` in a terminal once to remember your answer"
        ));
    }
    eprint!("qlang.toml wants to run these commands:\n{shown}Run them, and remember? [y/N] ");
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| e.to_string())?;
    if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        return Err("not trusted: nothing was run".to_string());
    }
    if let Some(file) = trust_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut text = std::fs::read_to_string(&file).unwrap_or_default();
        for s in &unknown {
            text.push_str(&trust_line(s).expect("checked above"));
            text.push('\n');
        }
        let _ = std::fs::write(&file, text);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_split_on_spaces_and_quotes() {
        assert_eq!(
            split_command(r#"python3 "my script.py" --x 'a b'"#).unwrap(),
            vec!["python3", "my script.py", "--x", "a b"]
        );
        assert_eq!(
            split_command(r"C:\tools\p.exe a").unwrap(),
            vec![r"C:\tools\p.exe", "a"]
        );
        assert!(split_command("a 'b").is_err());
        assert_eq!(split_command("  ").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn values_round_trip_through_json() {
        let v = HostValue::Map(vec![
            (HostValue::Str("n".into()), HostValue::Int(2)),
            (
                HostValue::Str("xs".into()),
                HostValue::Array(vec![
                    HostValue::Float(1.5),
                    HostValue::Null,
                    HostValue::Bool(true),
                ]),
            ),
        ]);
        assert_eq!(from_json(&to_json(&v)), v);
    }
}
