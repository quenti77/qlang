//! `qlang lint`: what the type checker does not say about a program.
//!
//! L001 an unused variable, L002 an unused private function, L003 an unused import, L004 code after
//! `return` / `break` / `continue`, L005 a function with too many paths (`--max-complexity`).

use qlang_core::ast::{Block, Expr, ExprKind, ImportKind, Item, StmtKind};
use qlang_core::check::{CompileOptions, compile_with};
use qlang_core::lexer::{Tok, lex};
use qlang_core::parser::parse_module;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const SKIPPED_DIRS: [&str; 3] = ["target", "node_modules", "dist"];
const DEFAULT_COMPLEXITY: usize = 15;
/// Names a framework or a runner looks up by name: never reported as unused.
const ENTRY_POINTS: [&str; 4] = ["run", "handle", "main", "setup"];

struct Finding {
    line: u32,
    col: u32,
    code: &'static str,
    text: String,
}

fn usage(why: &str) -> ExitCode {
    eprintln!("qlang lint: {why}\nusage: qlang lint [path...] [--max-complexity N]");
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

pub fn run(args: &[&str]) -> ExitCode {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut max_complexity = DEFAULT_COMPLEXITY;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--max-complexity" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => max_complexity = n,
                    None => return usage("--max-complexity needs a number"),
                }
            }
            a if a.starts_with('-') => return usage(&format!("unknown option `{a}`")),
            a => paths.push(PathBuf::from(a)),
        }
        i += 1;
    }
    let mut files: Vec<PathBuf> = Vec::new();
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
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|c| c.canonicalize().ok());
    let (mut total, mut broken) = (0usize, false);
    for file in &files {
        let shown = file
            .canonicalize()
            .ok()
            .and_then(|f| {
                cwd.as_ref()
                    .and_then(|c| f.strip_prefix(c).ok().map(Path::to_path_buf))
            })
            .unwrap_or_else(|| file.clone())
            .display()
            .to_string();
        match lint_file(file, &shown, max_complexity) {
            Ok(mut findings) => {
                findings.sort_by_key(|f| (f.line, f.col));
                for f in &findings {
                    println!(
                        "{shown}:{}:{}: warning[{}]: {}",
                        f.line, f.col, f.code, f.text
                    );
                }
                total += findings.len();
            }
            Err(e) => {
                eprintln!("{e}");
                broken = true;
            }
        }
    }
    if broken {
        ExitCode::from(65)
    } else if total > 0 {
        eprintln!("{total} finding(s)");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn lint_file(file: &Path, shown: &str, max_complexity: usize) -> Result<Vec<Finding>, String> {
    let source =
        std::fs::read_to_string(file).map_err(|e| format!("{shown}: cannot read it: {e}"))?;
    let mut host = crate::new_host_with(file, None, Some(0), Vec::new(), &[], true, false)?;
    let program = compile_with(shown, &source, &mut host, &CompileOptions { info: true })
        .map_err(|e| format!("{shown} does not compile:\n{}", e.render()))?;
    let info = program.info.ok_or("no information from the compiler")?;
    let mut out: Vec<Finding> = Vec::new();
    let mut diags = Vec::new();
    let module = parse_module(&source, 0, &mut diags);
    // an annotated function (a route, a provider...) is found by the framework, not by a call
    let annotated: Vec<&str> = module
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fun(f) if !f.annotations.is_empty() => f.name.as_ref().map(|n| n.name.as_str()),
            _ => None,
        })
        .collect();

    // what the type checker knows: names that nothing refers to
    let used = |def: &qlang_core::span::Location| info.refs.iter().any(|r| &r.def == def);
    for l in info.locals.iter().filter(|l| l.loc.file == shown) {
        if l.kind != "param" && !l.name.starts_with('_') && !used(&l.loc) {
            out.push(Finding {
                line: l.loc.line,
                col: l.loc.col,
                code: "L001",
                text: format!(
                    "`{}` is never used (start the name with `_` to keep it)",
                    l.name
                ),
            });
        }
    }
    for it in info.items.iter().filter(|it| it.loc.file == shown) {
        if it.kind == "function"
            && !it.public
            && !it.name.starts_with('_')
            && !it.name.starts_with("test_")
            && !ENTRY_POINTS.contains(&it.name.as_str())
            && !annotated.contains(&it.name.as_str())
            && !used(&it.loc)
        {
            out.push(Finding {
                line: it.loc.line,
                col: it.loc.col,
                code: "L002",
                text: format!("function `{}` is never used and not exported", it.name),
            });
        }
    }

    // what the tree and the tokens say
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let at = |offset: u32| -> (u32, u32) {
        let o = offset as usize;
        let line = line_starts.partition_point(|&s| s <= o).max(1);
        let col = source
            .get(line_starts[line - 1]..o.min(source.len()))
            .map_or(0, |s| s.chars().count());
        (line as u32, col as u32 + 1)
    };
    let tokens = lex(&source, 0, 0, &mut diags);
    let mut funs = Vec::new();
    for item in &module.items {
        match item {
            Item::Fun(f) => funs.push(f.clone()),
            Item::Impl(i) => funs.extend(i.methods.iter().map(|m| m.fun.clone())),
            _ => {}
        }
        if let Item::Import(imp) = item {
            let names: Vec<(&str, u32)> = match &imp.kind {
                ImportKind::Names(ns) => ns
                    .iter()
                    .map(|n| {
                        let id = n.alias.as_ref().unwrap_or(&n.name);
                        (id.name.as_str(), id.span.start)
                    })
                    .collect(),
                // a module imported as a whole may be there for what its top level does
                ImportKind::Module(_) => Vec::new(),
            };
            for (name, start) in names {
                if !mentioned(&tokens, name, imp.span.start, imp.span.end) {
                    let (line, col) = at(start);
                    out.push(Finding {
                        line,
                        col,
                        code: "L003",
                        text: format!("`{name}` is imported but never used"),
                    });
                }
            }
        }
        if let Item::Stmt(s) = item {
            unreachable_in_stmt(&s.kind, &at, &mut out);
        }
    }
    for f in funs {
        let Some(body) = &f.body else { continue };
        unreachable_in(body, &at, &mut out);
        let paths = 1 + tokens
            .iter()
            .filter(|t| t.span.start >= body.span.start && t.span.end <= body.span.end)
            .filter(|t| {
                matches!(
                    t.tok,
                    Tok::If | Tok::ElseIf | Tok::While | Tok::For | Tok::Case | Tok::And | Tok::Or
                )
            })
            .count();
        if paths > max_complexity {
            let name = f.name.as_ref().map_or("(anonymous)", |n| n.name.as_str());
            let (line, col) = at(f.name.as_ref().map_or(f.span.start, |n| n.span.start));
            out.push(Finding {
                line,
                col,
                code: "L005",
                text: format!(
                    "function `{name}` has {paths} paths (more than {max_complexity}): split it"
                ),
            });
        }
    }
    Ok(out)
}

/// Is `name` written outside `[skip_start, skip_end)`: as a name, or inside a string (`"{name}"`)?
fn mentioned(
    tokens: &[qlang_core::lexer::Token],
    name: &str,
    skip_start: u32,
    skip_end: u32,
) -> bool {
    tokens.iter().any(|t| {
        if t.span.start >= skip_start && t.span.end <= skip_end {
            return false;
        }
        match &t.tok {
            Tok::Ident(n) => n == name,
            Tok::Str(raw) => raw.match_indices(name).any(|(i, _)| {
                let before = raw[..i].chars().next_back();
                let after = raw[i + name.len()..].chars().next();
                let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
                !word(before) && !word(after)
            }),
            _ => false,
        }
    })
}

fn unreachable_in(block: &Block, at: &dyn Fn(u32) -> (u32, u32), out: &mut Vec<Finding>) {
    for (i, s) in block.stmts.iter().enumerate() {
        if matches!(
            s.kind,
            StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue
        ) && let Some(next) = block.stmts.get(i + 1)
        {
            let (line, col) = at(next.span.start);
            out.push(Finding {
                line,
                col,
                code: "L004",
                text: "this code is never reached (it follows return, break or continue)".into(),
            });
            break;
        }
        unreachable_in_stmt(&s.kind, at, out);
    }
}

fn unreachable_in_stmt(kind: &StmtKind, at: &dyn Fn(u32) -> (u32, u32), out: &mut Vec<Finding>) {
    match kind {
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => unreachable_in(body, at, out),
        StmtKind::Expr(e) | StmtKind::Let { init: e, .. } => unreachable_in_expr(e, at, out),
        StmtKind::Seq { stmts, .. } => {
            for s in stmts {
                unreachable_in_stmt(&s.kind, at, out);
            }
        }
        _ => {}
    }
}

fn unreachable_in_expr(e: &Expr, at: &dyn Fn(u32) -> (u32, u32), out: &mut Vec<Finding>) {
    match &e.kind {
        ExprKind::If {
            branches,
            else_block,
        } => {
            for (_, b) in branches {
                unreachable_in(b, at, out);
            }
            if let Some(b) = else_block {
                unreachable_in(b, at, out);
            }
        }
        ExprKind::Match {
            cases, else_block, ..
        } => {
            for c in cases {
                unreachable_in(&c.body, at, out);
            }
            if let Some(b) = else_block {
                unreachable_in(b, at, out);
            }
        }
        ExprKind::Lambda(f) => {
            if let Some(b) = &f.body {
                unreachable_in(b, at, out);
            }
        }
        _ => {}
    }
}
