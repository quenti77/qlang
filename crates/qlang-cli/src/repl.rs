//! `qlang repl`: type qlang, see the result.
//!
//! Every input is compiled together with what the session defined before (functions, types,
//! imports and `let`s), so a definition stays; an expression is run once and its value shown. The
//! initializer of a `let` runs again with each later input: keep side effects out of it.

use qlang_core::ast::{Item, StmtKind};
use qlang_core::check::{CompileOptions, compile, compile_with};
use qlang_core::host::Limits;
use qlang_core::parser::parse_module;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const KEYWORDS: [&str; 38] = [
    "let",
    "const",
    "fun",
    "return",
    "if",
    "then",
    "elseif",
    "else",
    "end",
    "while",
    "for",
    "in",
    "step",
    "do",
    "break",
    "continue",
    "match",
    "case",
    "struct",
    "impl",
    "trait",
    "enum",
    "extends",
    "override",
    "public",
    "private",
    "protected",
    "static",
    "import",
    "export",
    "extern",
    "from",
    "as",
    "and",
    "or",
    "not",
    "true",
    "false",
];

/// What the session can complete: words, and the members of types.
#[derive(Default)]
struct Names {
    words: Vec<String>,
    members: Vec<(String, Vec<String>)>,
}

struct Completion(Names);

impl Completer for Completion {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let head = &line[..pos];
        let start = head
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        let prefix = &head[start..];
        let pair = |w: &str| Pair {
            display: w.to_string(),
            replacement: w.to_string(),
        };
        // `Json.pa|`: the members of `Json`
        if start > 0 && head[..start].ends_with('.') {
            let owner_end = start - 1;
            let owner_start = head[..owner_end]
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let owner = &head[owner_start..owner_end];
            let found = self
                .0
                .members
                .iter()
                .find(|(n, _)| n == owner)
                .map(|(_, ms)| {
                    ms.iter()
                        .filter(|m| m.starts_with(prefix))
                        .map(|m| pair(m))
                        .collect()
                })
                .unwrap_or_default();
            return Ok((start, found));
        }
        let mut out: Vec<Pair> = self
            .0
            .words
            .iter()
            .filter(|w| w.starts_with(prefix) && !prefix.is_empty())
            .map(|w| pair(w))
            .collect();
        out.sort_by(|a, b| a.display.cmp(&b.display));
        out.dedup_by(|a, b| a.display == b.display);
        Ok((start, out))
    }
}

impl Hinter for Completion {
    type Hint = String;
}
impl Highlighter for Completion {}
impl Validator for Completion {}
impl Helper for Completion {}

struct Session {
    /// The definitions so far, as source text.
    defined: String,
    file: PathBuf,
    allow_process: bool,
    limits: Limits,
}

/// The input is unfinished when the error is at its very end (a missing `end`, an open bracket).
fn unfinished(source: &str, chunk: &str) -> bool {
    let mut diags = Vec::new();
    let _ = parse_module(chunk, 0, &mut diags);
    let end = chunk.trim_end().len() as u32;
    let _ = source;
    diags
        .iter()
        .any(|d| d.code.starts_with('P') && d.span.start >= end.saturating_sub(1))
}

impl Session {
    fn host(&self) -> Result<crate::TerminalHost, String> {
        let mut host = crate::new_host(&self.file, None, None, Vec::new(), &[], true)?;
        host.allow_process = self.allow_process;
        Ok(host)
    }

    /// Does the chunk only define things (functions, types, imports, variables)?
    fn defines(chunk: &str) -> bool {
        let mut diags = Vec::new();
        let module = parse_module(chunk, 0, &mut diags);
        !module.items.is_empty()
            && module.items.iter().all(|i| match i {
                Item::Stmt(s) => matches!(s.kind, StmtKind::Let { .. } | StmtKind::LetTuple { .. }),
                _ => true,
            })
    }

    fn run_source(&self, source: &str) -> Result<(), String> {
        let mut host = self.host()?;
        let shown = "repl";
        let program = compile(shown, source, &mut host).map_err(|e| e.render())?;
        let result = program.run(
            &mut host,
            qlang_core::host::Limits {
                max_steps: self.limits.max_steps,
                max_depth: self.limits.max_depth,
                max_output: self.limits.max_output,
                max_alloc: self.limits.max_alloc,
            },
        );
        crate::term::restore();
        match result.error {
            Some(d) => Err(d.render(&program.sources)),
            None => Ok(()),
        }
    }

    /// Run one complete input.
    fn eval(&mut self, chunk: &str) -> Result<(), String> {
        if Self::defines(chunk) {
            // checked with everything before it, and run once so that a failing `let` is not kept
            let source = format!("{}{chunk}\n", self.defined);
            self.run_source(&source)?;
            self.defined = source;
            return Ok(());
        }
        // a value is shown; a call that returns nothing (`print(1)`) is just run
        let shown = format!("{}print({})\n", self.defined, chunk.trim());
        let mut host = self.host()?;
        if compile("repl", &shown, &mut host).is_ok() {
            return self.run_source(&shown);
        }
        self.run_source(&format!("{}{chunk}\n", self.defined))
    }

    fn names(&self) -> Names {
        let mut host = match self.host() {
            Ok(h) => h,
            Err(_) => return Names::default(),
        };
        let program = compile_with(
            "repl",
            &self.defined,
            &mut host,
            &CompileOptions { info: true },
        );
        let Ok(program) = program else {
            return Names::default();
        };
        let Some(info) = program.info else {
            return Names::default();
        };
        let mut words: Vec<String> = KEYWORDS.iter().map(|s| s.to_string()).collect();
        words.extend(info.functions.iter().map(|m| m.name.clone()));
        words.extend(info.statics.keys().filter(|k| !k.contains(' ')).cloned());
        if let Some(g) = info.globals.get("repl") {
            words.extend(g.iter().map(|m| m.name.clone()));
        }
        let members = info
            .statics
            .iter()
            .map(|(k, ms)| (k.clone(), ms.iter().map(|m| m.name.clone()).collect()))
            .collect();
        Names { words, members }
    }
}

fn history_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(Path::new(&home).join(".qlang_history"))
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut allow_process = false;
    let mut limits = Limits::default();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--allow-process" => allow_process = true,
            "--max-steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => limits.max_steps = n,
                    None => {
                        eprintln!("--max-steps needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            a => {
                eprintln!(
                    "qlang repl: unknown option `{a}`\nusage: qlang repl [--allow-process] [--max-steps N]"
                );
                return ExitCode::from(64);
            }
        }
        i += 1;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut session = Session {
        defined: String::new(),
        file: cwd.join("repl.q"),
        allow_process,
        limits,
    };
    if !std::io::stdin().is_terminal() {
        // a pipe or a file: no line editor, no banner, one line after the other
        let mut chunk = String::new();
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if chunk.is_empty() && line.trim().is_empty() {
                continue;
            }
            chunk.push_str(&line);
            chunk.push('\n');
            if unfinished(&session.defined, &chunk) {
                continue;
            }
            if let Err(e) = session.eval(&chunk) {
                eprint!("{e}");
                if !e.ends_with('\n') {
                    eprintln!();
                }
            }
            chunk.clear();
        }
        return ExitCode::SUCCESS;
    }
    let mut editor: Editor<Completion, rustyline::history::FileHistory> = match Editor::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("qlang repl: {e}");
            return ExitCode::from(1);
        }
    };
    editor.set_helper(Some(Completion(session.names())));
    let history = history_file();
    if let Some(h) = &history {
        let _ = editor.load_history(h);
    }
    println!(
        "qlang {} (ctrl-D to leave; definitions stay, an expression shows its value)",
        env!("CARGO_PKG_VERSION")
    );
    let mut chunk = String::new();
    loop {
        let prompt = if chunk.is_empty() { "> " } else { ". " };
        match editor.readline(prompt) {
            Ok(line) => {
                if chunk.is_empty() && line.trim().is_empty() {
                    continue;
                }
                chunk.push_str(&line);
                chunk.push('\n');
                if unfinished(&session.defined, &chunk) {
                    continue;
                }
                let _ = editor.add_history_entry(chunk.trim_end());
                if let Err(e) = session.eval(&chunk) {
                    eprint!("{e}");
                    if !e.ends_with('\n') {
                        eprintln!();
                    }
                }
                chunk.clear();
                if let Some(h) = editor.helper_mut() {
                    *h = Completion(session.names());
                }
            }
            Err(ReadlineError::Interrupted) => {
                chunk.clear();
                println!("(input dropped; ctrl-D to leave)");
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("qlang repl: {e}");
                break;
            }
        }
    }
    if let Some(h) = &history {
        let _ = editor.save_history(h);
    }
    ExitCode::SUCCESS
}
