//! `qlang dap`: a debugger for editors, over the Debug Adapter Protocol (standard input and output).
//!
//! The program runs on a thread of its own with the tree walker; before every statement it asks a
//! hook whether to stop (breakpoint, step, pause). While stopped, the editor reads a snapshot made of
//! text, so nothing of the program crosses between threads.

use qlang_core::check::compile;
use qlang_core::debug::{Action, Hook, Snapshot};
use qlang_core::host::Limits;
use serde_json::{Value as J, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Run,
    StepIn,
    Next,
    StepOut,
}

struct State {
    breakpoints: HashMap<PathBuf, HashSet<u32>>,
    pause: bool,
    mode: Mode,
    /// Depth and position where the program last stopped.
    from_depth: usize,
    from: (PathBuf, u32),
    reason: &'static str,
    stopped: Option<Snapshot>,
    resume: Option<Mode>,
    quit: bool,
}

/// A path without the `\\?\` prefix Windows gives to canonical paths (it breaks the way modules are looked up).
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

fn canonical(path: &Path) -> PathBuf {
    plain(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    out: Mutex<Box<dyn Write + Send>>,
    seq: AtomicI64,
}

impl Shared {
    fn send(&self, mut message: J) {
        message["seq"] = json!(self.seq.fetch_add(1, Ordering::SeqCst));
        let body = message.to_string();
        let mut out = self.out.lock().unwrap();
        let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
        let _ = out.flush();
    }

    fn event(&self, name: &str, body: J) {
        self.send(json!({"type": "event", "event": name, "body": body}));
    }
}

struct DebugHook {
    shared: Arc<Shared>,
    /// The directory file names are relative to.
    base: PathBuf,
    paths: HashMap<String, PathBuf>,
    depth: usize,
    at: (PathBuf, u32),
}

impl DebugHook {
    fn resolve(&mut self, file: &str) -> PathBuf {
        if let Some(p) = self.paths.get(file) {
            return p.clone();
        }
        let raw = Path::new(file);
        let full = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.base.join(raw)
        };
        let full = canonical(&full);
        self.paths.insert(file.to_string(), full.clone());
        full
    }
}

impl Hook for DebugHook {
    fn check(&mut self, file: &str, line: u32, depth: usize) -> Action {
        let path = self.resolve(file);
        self.depth = depth;
        self.at = (path.clone(), line);
        let mut st = self.shared.state.lock().unwrap();
        if st.quit {
            return Action::Abort;
        }
        let moved = st.from != (path.clone(), line) || st.from_depth != depth;
        let reason = if st.pause {
            st.pause = false;
            Some("pause")
        } else if st.breakpoints.get(&path).is_some_and(|l| l.contains(&line)) {
            Some("breakpoint")
        } else {
            match st.mode {
                Mode::Run => None,
                Mode::StepIn => moved.then_some("step"),
                Mode::Next => (moved && depth <= st.from_depth).then_some("step"),
                Mode::StepOut => (depth < st.from_depth).then_some("step"),
            }
        };
        match reason {
            Some(r) => {
                st.reason = r;
                Action::Stop
            }
            None => Action::Continue,
        }
    }

    fn stopped(&mut self, snapshot: Snapshot) -> bool {
        let mut st = self.shared.state.lock().unwrap();
        st.from = self.at.clone();
        st.from_depth = self.depth;
        st.stopped = Some(snapshot);
        let reason = st.reason;
        drop(st);
        self.shared.event(
            "stopped",
            json!({"reason": reason, "threadId": 1, "allThreadsStopped": true}),
        );
        let mut st = self.shared.state.lock().unwrap();
        loop {
            if st.quit {
                return false;
            }
            if let Some(mode) = st.resume.take() {
                st.mode = mode;
                st.stopped = None;
                return true;
            }
            st = self.shared.wake.wait(st).unwrap();
        }
    }
}

fn read_message(input: &mut impl BufRead) -> Option<J> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            length = v.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; length?];
    input.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

struct Launch {
    program: PathBuf,
    args: Vec<String>,
    stop_on_entry: bool,
    allow_process: bool,
}

fn run_program(shared: Arc<Shared>, launch: Launch) {
    let sink = {
        let shared = shared.clone();
        Arc::new(move |is_err: bool, text: &str| {
            shared.event(
                "output",
                json!({"category": if is_err { "stderr" } else { "stdout" }, "output": text}),
            );
        })
    };
    let finish = |code: i32| {
        shared.event("exited", json!({"exitCode": code}));
        shared.event("terminated", json!({}));
    };
    let source = match std::fs::read_to_string(&launch.program) {
        Ok(s) => s,
        Err(e) => {
            sink(
                true,
                &format!("cannot read {}: {e}\n", launch.program.display()),
            );
            return finish(66);
        }
    };
    let mut host =
        match crate::new_host(&launch.program, None, None, launch.args.clone(), &[], true) {
            Ok(h) => h,
            Err(e) => {
                sink(true, &format!("{e}\n"));
                return finish(1);
            }
        };
    host.allow_process = launch.allow_process;
    host.sink = Some(sink.clone());
    let shown = launch.program.display().to_string();
    let program = match compile(&shown, &source, &mut host) {
        Ok(p) => p,
        Err(e) => {
            sink(true, &e.render());
            return finish(1);
        }
    };
    let base = launch
        .program
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    {
        let mut st = shared.state.lock().unwrap();
        st.mode = if launch.stop_on_entry {
            Mode::StepIn
        } else {
            Mode::Run
        };
        st.reason = "entry";
    }
    qlang_core::debug::install(Box::new(DebugHook {
        shared: shared.clone(),
        base,
        paths: HashMap::new(),
        depth: 0,
        at: (PathBuf::new(), 0),
    }));
    let result = program.run(&mut host, Limits::default());
    match result.error {
        Some(d) => {
            // ending the session on purpose is not an error worth showing
            if d.code != "R907" {
                sink(true, &d.render(&program.sources));
            }
            finish(2)
        }
        None => finish(result.exit_code.unwrap_or(0)),
    }
}

pub fn run() -> ExitCode {
    let stdout: Box<dyn Write + Send> = Box::new(std::io::stdout());
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            breakpoints: HashMap::new(),
            pause: false,
            mode: Mode::Run,
            from_depth: usize::MAX,
            from: (PathBuf::new(), 0),
            reason: "breakpoint",
            stopped: None,
            resume: None,
            quit: false,
        }),
        wake: Condvar::new(),
        out: Mutex::new(stdout),
        seq: AtomicI64::new(1),
    });
    let mut input = std::io::BufReader::new(std::io::stdin());
    let mut launch: Option<Launch> = None;
    while let Some(request) = read_message(&mut input) {
        let command = request["command"].as_str().unwrap_or("").to_string();
        let args = request["arguments"].clone();
        let reply = |ok: bool, body: J, message: Option<&str>| {
            let mut r = json!({
                "type": "response",
                "request_seq": request["seq"],
                "command": command,
                "success": ok,
                "body": body,
            });
            if let Some(m) = message {
                r["message"] = json!(m);
            }
            shared.send(r);
        };
        match command.as_str() {
            "initialize" => {
                reply(
                    true,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsEvaluateForHovers": true,
                        "supportsTerminateRequest": true,
                    }),
                    None,
                );
                shared.event("initialized", json!({}));
            }
            "launch" => {
                let Some(program) = args["program"].as_str() else {
                    reply(
                        false,
                        json!({}),
                        Some("`program` is missing from the launch configuration"),
                    );
                    continue;
                };
                let program = PathBuf::from(program);
                let program = canonical(&program);
                launch = Some(Launch {
                    program,
                    args: args["args"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    stop_on_entry: args["stopOnEntry"].as_bool().unwrap_or(false),
                    allow_process: args["allowProcess"].as_bool().unwrap_or(false),
                });
                if let Some(cwd) = args["cwd"].as_str() {
                    let _ = std::env::set_current_dir(cwd);
                }
                reply(true, json!({}), None);
            }
            "setBreakpoints" => {
                let path = args["source"]["path"].as_str().unwrap_or("");
                let path = canonical(Path::new(path));
                let lines: Vec<u32> = args["breakpoints"]
                    .as_array()
                    .map(|b| {
                        b.iter()
                            .filter_map(|x| x["line"].as_u64())
                            .map(|l| l as u32)
                            .collect()
                    })
                    .unwrap_or_default();
                shared
                    .state
                    .lock()
                    .unwrap()
                    .breakpoints
                    .insert(path, lines.iter().copied().collect());
                let list: Vec<J> = lines
                    .iter()
                    .map(|l| json!({"verified": true, "line": l}))
                    .collect();
                reply(true, json!({"breakpoints": list}), None);
            }
            "setExceptionBreakpoints" => reply(true, json!({}), None),
            "configurationDone" => {
                reply(true, json!({}), None);
                if let Some(l) = launch.take() {
                    let shared = shared.clone();
                    let _ = std::thread::Builder::new()
                        .name("program".into())
                        .stack_size(256 << 20)
                        .spawn(move || run_program(shared, l));
                }
            }
            "threads" => reply(true, json!({"threads": [{"id": 1, "name": "main"}]}), None),
            "stackTrace" => {
                let st = shared.state.lock().unwrap();
                let frames: Vec<J> = st
                    .stopped
                    .as_ref()
                    .map(|s| {
                        s.frames
                            .iter()
                            .enumerate()
                            .map(|(i, f)| {
                                let path = Path::new(&f.file);
                                json!({
                                    "id": i,
                                    "name": f.name,
                                    "source": {"name": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "path": f.file},
                                    "line": f.line,
                                    "column": f.column,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let total = frames.len();
                reply(
                    true,
                    json!({"stackFrames": frames, "totalFrames": total}),
                    None,
                );
            }
            "scopes" => {
                let id = args["frameId"].as_u64().unwrap_or(0) as usize;
                let st = shared.state.lock().unwrap();
                let scopes: Vec<J> = st
                    .stopped
                    .as_ref()
                    .and_then(|s| s.frames.get(id))
                    .map(|f| {
                        f.scopes
                            .iter()
                            .map(|s| json!({"name": s.name, "variablesReference": s.reference, "expensive": false}))
                            .collect()
                    })
                    .unwrap_or_default();
                reply(true, json!({"scopes": scopes}), None);
            }
            "variables" => {
                let r = args["variablesReference"].as_u64().unwrap_or(0) as usize;
                let st = shared.state.lock().unwrap();
                let vars: Vec<J> = st
                    .stopped
                    .as_ref()
                    .and_then(|s| s.children.get(r.wrapping_sub(1)))
                    .map(|list| {
                        list.iter()
                            .map(|v| json!({"name": v.name, "value": v.value, "type": v.ty, "variablesReference": v.reference}))
                            .collect()
                    })
                    .unwrap_or_default();
                reply(true, json!({"variables": vars}), None);
            }
            "evaluate" => {
                let expression = args["expression"].as_str().unwrap_or("").trim().to_string();
                let frame = args["frameId"].as_u64().unwrap_or(0) as usize;
                let st = shared.state.lock().unwrap();
                match st
                    .stopped
                    .as_ref()
                    .and_then(|s| lookup(s, frame, &expression))
                {
                    Some(v) => reply(
                        true,
                        json!({"result": v.value, "type": v.ty, "variablesReference": v.reference}),
                        None,
                    ),
                    None => reply(
                        false,
                        json!({}),
                        Some(
                            "only variables can be looked up here (`name`, `name.field`, `name.0`)",
                        ),
                    ),
                }
            }
            "continue" | "next" | "stepIn" | "stepOut" => {
                let mode = match command.as_str() {
                    "next" => Mode::Next,
                    "stepIn" => Mode::StepIn,
                    "stepOut" => Mode::StepOut,
                    _ => Mode::Run,
                };
                {
                    let mut st = shared.state.lock().unwrap();
                    st.resume = Some(mode);
                }
                shared.wake.notify_all();
                reply(true, json!({"allThreadsContinued": true}), None);
            }
            "pause" => {
                shared.state.lock().unwrap().pause = true;
                reply(true, json!({}), None);
            }
            "disconnect" | "terminate" => {
                shared.state.lock().unwrap().quit = true;
                shared.wake.notify_all();
                reply(true, json!({}), None);
                // the program may be inside a long wait: do not wait for it
                std::thread::sleep(std::time::Duration::from_millis(50));
                return ExitCode::SUCCESS;
            }
            _ => reply(false, json!({}), Some("not supported by `qlang dap`")),
        }
    }
    // the editor went away
    shared.state.lock().unwrap().quit = true;
    shared.wake.notify_all();
    ExitCode::SUCCESS
}

/// `a`, `a.b`, `a.0`: a variable of the frame, looked up in its scopes.
fn lookup(s: &Snapshot, frame: usize, expression: &str) -> Option<qlang_core::debug::Var> {
    let f = s.frames.get(frame)?;
    let mut parts = expression.split('.');
    let first = parts.next()?;
    let mut found = f
        .scopes
        .iter()
        .filter_map(|sc| s.children.get(sc.reference.checked_sub(1)?))
        .flatten()
        .find(|v| v.name == first)?
        .clone();
    for part in parts {
        let list = s.children.get(found.reference.checked_sub(1)?)?;
        found = list.iter().find(|v| v.name == part)?.clone();
    }
    Some(found)
}
