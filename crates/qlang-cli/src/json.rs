//! The JSON protocol: one request in, one response out. No disk access:
//! imported modules come from the request itself.

use qlang_core::check::compile;
use qlang_core::diag::Diagnostic;
use qlang_core::host::{Limits, MemoryHost};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::process::ExitCode;

#[derive(Deserialize)]
struct Request {
    /// Name of the main file; defaults to `main.q` or the only file.
    entry: Option<String>,
    /// File name to source text. Imports are resolved among these.
    files: HashMap<String, String>,
    /// Lines for `read()`. If absent, `read` is not available.
    input: Option<Vec<String>>,
    limits: Option<LimitsRequest>,
    /// `"check"`: compile only (no execution). Default: `"run"`.
    mode: Option<String>,
}

#[derive(Deserialize, Default)]
struct LimitsRequest {
    max_steps: Option<u64>,
    max_depth: Option<usize>,
    max_output: Option<usize>,
    max_alloc: Option<usize>,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    /// `"request"`, `"compile"`, `"check"` or `"run"`: where it stopped (or finished).
    phase: &'static str,
    output: String,
    diagnostics: Vec<Diagnostic>,
    steps: u64,
}

fn failure(phase: &'static str, message: String) -> Response {
    Response {
        ok: false,
        phase,
        output: String::new(),
        diagnostics: vec![Diagnostic::error("J001", message, Default::default())],
        steps: 0,
    }
}

/// Upper bounds applied to what a request may ask for.
pub struct Ceiling {
    pub max_steps: u64,
    pub max_depth: usize,
    pub max_output: usize,
    pub max_alloc: usize,
}

impl Default for Ceiling {
    fn default() -> Self {
        let d = Limits::default();
        Ceiling { max_steps: d.max_steps, max_depth: d.max_depth, max_output: d.max_output, max_alloc: d.max_alloc }
    }
}

pub fn handle(request: &str, ceiling: &Ceiling) -> String {
    handle_with(request, ceiling, false)
}

pub fn handle_with(request: &str, ceiling: &Ceiling, force_check: bool) -> String {
    let resp = match serde_json::from_str::<Request>(request) {
        Ok(req) => execute(req, ceiling, force_check),
        Err(e) => failure("request", format!("invalid request: {e}")),
    };
    serde_json::to_string(&resp).unwrap_or_else(|_| "{\"ok\":false}".to_string())
}

fn execute(req: Request, ceiling: &Ceiling, force_check: bool) -> Response {
    let check_only = force_check || req.mode.as_deref() == Some("check");
    let entry = match req.entry {
        Some(e) => e,
        None if req.files.len() == 1 => req.files.keys().next().unwrap().clone(),
        None => "main.q".to_string(),
    };
    let Some(source) = req.files.get(&entry).cloned() else {
        return failure("request", format!("the entry file \"{entry}\" is not in `files`"));
    };
    let mut host = MemoryHost { files: req.files, allow_read: req.input.is_some(), ..MemoryHost::default() };
    if let Some(lines) = req.input {
        host.input = lines.into();
    }
    let mut limits = Limits::default();
    if let Some(l) = req.limits {
        limits.max_steps = l.max_steps.unwrap_or(limits.max_steps).min(ceiling.max_steps);
        limits.max_depth = l.max_depth.unwrap_or(limits.max_depth).min(ceiling.max_depth);
        limits.max_output = l.max_output.unwrap_or(limits.max_output).min(ceiling.max_output);
        limits.max_alloc = l.max_alloc.unwrap_or(limits.max_alloc).min(ceiling.max_alloc);
    }
    limits.max_steps = limits.max_steps.min(ceiling.max_steps);
    limits.max_depth = limits.max_depth.min(ceiling.max_depth);
    limits.max_output = limits.max_output.min(ceiling.max_output);
    limits.max_alloc = limits.max_alloc.min(ceiling.max_alloc);

    let program = match compile(&entry, &source, &mut host) {
        Ok(p) => p,
        Err(e) => {
            return Response { ok: false, phase: "compile", output: String::new(), diagnostics: e.diagnostics, steps: 0 };
        }
    };
    if check_only {
        return Response { ok: true, phase: "check", output: String::new(), diagnostics: program.warnings.clone(), steps: 0 };
    }
    let result = program.run(&mut host, limits);
    let mut diagnostics = program.warnings.clone();
    let ok = result.error.is_none();
    diagnostics.extend(result.error);
    Response { ok, phase: "run", output: host.output, diagnostics, steps: result.steps }
}

pub fn run_stdin(force_check: bool) -> ExitCode {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("cannot read the request: {e}");
        return ExitCode::from(66);
    }
    println!("{}", handle_with(&input, &Ceiling::default(), force_check));
    ExitCode::SUCCESS
}
