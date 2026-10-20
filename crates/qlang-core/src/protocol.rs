//! The JSON protocol: one request in, one response out. No disk access: imported
//! modules come from the request itself. Used by the command line, the HTTP server and the
//! WebAssembly build.

use crate::check::{CompileOptions, Info, compile_with};
use crate::diag::Diagnostic;
use crate::host::{Limits, MemoryHost};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
struct Request {
    /// Name of the main file; defaults to `main.q` or the only file.
    entry: Option<String>,
    /// File name to source text. Imports are resolved among these.
    files: HashMap<String, String>,
    /// Lines for `read()`. If absent, `read` is not available.
    input: Option<Vec<String>>,
    /// The current time in milliseconds since 1970, for `time.now()`. If absent, there is no clock.
    now: Option<i64>,
    /// Seed of the random generator (same seed, same numbers). If absent, there is no randomness.
    seed: Option<u64>,
    limits: Option<LimitsRequest>,
    /// `"check"`: compile only (no execution). Default: `"run"`.
    mode: Option<String>,
    /// Also answer what an editor needs: types, local variables, members.
    info: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<Info>,
}

fn failure(phase: &'static str, message: String) -> Response {
    Response {
        ok: false,
        phase,
        output: String::new(),
        diagnostics: vec![Diagnostic::error("J001", message, Default::default())],
        steps: 0,
        info: None,
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
        Ceiling {
            max_steps: d.max_steps,
            max_depth: d.max_depth,
            max_output: d.max_output,
            max_alloc: d.max_alloc,
        }
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
        return failure(
            "request",
            format!("the entry file \"{entry}\" is not in `files`"),
        );
    };
    let mut host = MemoryHost {
        files: req.files,
        allow_read: req.input.is_some(),
        clock: req.now,
        seed: req.seed,
        // a check cannot know what the real host provides: it does not report it
        assume_externs: check_only,
        ..MemoryHost::default()
    };
    if let Some(lines) = req.input {
        host.input = lines.into();
    }
    let mut limits = Limits::default();
    if let Some(l) = req.limits {
        limits.max_steps = l
            .max_steps
            .unwrap_or(limits.max_steps)
            .min(ceiling.max_steps);
        limits.max_depth = l
            .max_depth
            .unwrap_or(limits.max_depth)
            .min(ceiling.max_depth);
        limits.max_output = l
            .max_output
            .unwrap_or(limits.max_output)
            .min(ceiling.max_output);
        limits.max_alloc = l
            .max_alloc
            .unwrap_or(limits.max_alloc)
            .min(ceiling.max_alloc);
    }
    limits.max_steps = limits.max_steps.min(ceiling.max_steps);
    limits.max_depth = limits.max_depth.min(ceiling.max_depth);
    limits.max_output = limits.max_output.min(ceiling.max_output);
    limits.max_alloc = limits.max_alloc.min(ceiling.max_alloc);

    let options = CompileOptions {
        info: req.info.unwrap_or(false),
    };
    let program = match compile_with(&entry, &source, &mut host, &options) {
        Ok(p) => p,
        Err(e) => {
            return Response {
                ok: false,
                phase: "compile",
                output: String::new(),
                diagnostics: e.diagnostics,
                steps: 0,
                info: e.info.map(|i| *i),
            };
        }
    };
    if check_only {
        return Response {
            ok: true,
            phase: "check",
            output: String::new(),
            diagnostics: program.warnings.clone(),
            steps: 0,
            info: program.info,
        };
    }
    let result = program.run(&mut host, limits);
    let mut diagnostics = program.warnings.clone();
    let ok = result.error.is_none();
    diagnostics.extend(result.error);
    Response {
        ok,
        phase: "run",
        output: host.output,
        diagnostics,
        steps: result.steps,
        info: None,
    }
}
