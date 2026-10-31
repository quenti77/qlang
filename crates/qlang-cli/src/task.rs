//! `Worker`: a file run at the same time as the program, by an interpreter that shares nothing with it.
//! The two talk with JSON texts. The program side keeps the tasks it started in a table; the task side
//! finds its own channels in a thread-local.

use qlang_core::check::compile;
use qlang_core::host::{HostError, HostValue, Limits};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// The step limit tasks get (0: the default one).
static MAX_STEPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `--max-steps` of the program, passed on to its tasks.
pub fn set_max_steps(n: u64) {
    MAX_STEPS.store(n, std::sync::atomic::Ordering::Relaxed);
}

pub const TASK_EXTERNS: &[&str] = &[
    "task_spawn",
    "task_send",
    "task_recv",
    "task_wait",
    "task_cancel",
    "task_done",
    "task_input",
    "task_emit",
    "task_next",
    "task_cancelled",
    "task_finish",
];

/// How many tasks may run at once in the process.
const MAX_TASKS: usize = 64;
/// The longest a call waits for a message or a result.
const MAX_WAIT_MS: i64 = 3_600_000;

type Outcome = Result<String, String>;

struct Handle {
    to_task: Sender<String>,
    from_task: Mutex<Receiver<String>>,
    cancel: Arc<AtomicBool>,
    outcome: Arc<(Mutex<Option<Outcome>>, Condvar)>,
}

static TASKS: Mutex<Vec<(u64, Arc<Handle>)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// What a task finds around it.
struct Inside {
    input: String,
    inbox: Receiver<String>,
    outbox: Sender<String>,
    cancel: Arc<AtomicBool>,
    result: Option<String>,
}

thread_local! {
    static INSIDE: RefCell<Option<Inside>> = const { RefCell::new(None) };
}

/// What the program that calls `Worker.spawn` allows its tasks.
pub struct Parent {
    pub root: Option<PathBuf>,
    pub allow_process: bool,
}

fn fail(message: impl Into<String>) -> HostError {
    HostError::Other(message.into())
}

fn find(id: i64) -> Result<Arc<Handle>, HostError> {
    let t = TASKS.lock().unwrap_or_else(|e| e.into_inner());
    t.iter()
        .find(|(i, _)| *i as i64 == id)
        .map(|(_, h)| h.clone())
        .ok_or_else(|| {
            fail("this task is over and its result was already taken (or there is no such task)")
        })
}

fn millis(ms: i64) -> Duration {
    Duration::from_millis(ms.clamp(0, MAX_WAIT_MS) as u64)
}

fn spawn(file: &str, input: &str, parent: &Parent) -> Result<i64, HostError> {
    let path = PathBuf::from(file);
    let full = path
        .canonicalize()
        .map_err(|e| fail(format!("cannot find the task file {file}: {e}")))?;
    if let Some(root) = &parent.root {
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        if !full.starts_with(&root) {
            return Err(fail(format!(
                "{file} is outside of --root {}",
                root.display()
            )));
        }
    }
    if RUNNING.fetch_add(1, Ordering::SeqCst) >= MAX_TASKS {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        return Err(fail(format!("at most {MAX_TASKS} tasks at once")));
    }
    let (to_task, inbox) = std::sync::mpsc::channel::<String>();
    let (outbox, from_task) = std::sync::mpsc::channel::<String>();
    let cancel = Arc::new(AtomicBool::new(false));
    let outcome: Arc<(Mutex<Option<Outcome>>, Condvar)> =
        Arc::new((Mutex::new(None), Condvar::new()));
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let handle = Arc::new(Handle {
        to_task,
        from_task: Mutex::new(from_task),
        cancel: cancel.clone(),
        outcome: outcome.clone(),
    });
    let inside = Inside {
        input: input.to_string(),
        inbox,
        outbox,
        cancel: cancel.clone(),
        result: None,
    };
    let root = parent.root.clone();
    let allow_process = parent.allow_process;
    let built = std::thread::Builder::new()
        .name(format!("qlang-task-{id}"))
        .stack_size(256 << 20)
        .spawn(move || {
            qlang_core::host::set_cancel_flag(Some(cancel));
            INSIDE.with(|i| *i.borrow_mut() = Some(inside));
            let result = run_file(&full, root, allow_process);
            let finished = INSIDE
                .with(|i| i.borrow_mut().take())
                .and_then(|i| i.result)
                .unwrap_or_else(|| "null".to_string());
            let (slot, cv) = &*outcome;
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.map(|()| finished));
            cv.notify_all();
            RUNNING.fetch_sub(1, Ordering::SeqCst);
        });
    if let Err(e) = built {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        return Err(fail(format!("cannot start the task: {e}")));
    }
    TASKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((id, handle));
    Ok(id as i64)
}

/// Compiles the file and runs its `fun run()`.
fn run_file(path: &Path, root: Option<PathBuf>, allow_process: bool) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut host = crate::new_host(path, root, None, Vec::new(), &[], false)?;
    host.allow_process = allow_process;
    let shown = path.display().to_string();
    let program = compile(&shown, &source, &mut host).map_err(|e| e.render())?;
    program
        .run_test("run", &mut host, {
            let mut limits = Limits::default();
            // a task is held to the same step limit as the program that started it (`--max-steps`)
            let steps = MAX_STEPS.load(std::sync::atomic::Ordering::Relaxed);
            if steps > 0 {
                limits.max_steps = steps;
            }
            limits
        })
        .map_err(|d| {
            if d.code == "R070" {
                format!("{shown} has no `fun run()` (the function a task starts with)")
            } else {
                d.render(&program.sources)
            }
        })
}

fn inside<T>(f: impl FnOnce(&mut Inside) -> T) -> Result<T, HostError> {
    INSIDE.with(|i| match i.borrow_mut().as_mut() {
        Some(inside) => Ok(f(inside)),
        None => Err(fail(
            "this only works inside a task (a file started by Worker.spawn)",
        )),
    })
}

fn stopping() -> bool {
    crate::stop::stopping(crate::serve::is_stopping())
}

pub fn call(name: &str, args: &[HostValue], parent: &Parent) -> Result<HostValue, HostError> {
    let wrong = || fail(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        ("task_spawn", [HostValue::Str(file), HostValue::Str(input)]) => {
            spawn(file, input, parent).map(HostValue::Int)
        }
        ("task_send", [HostValue::Int(id), HostValue::Str(message)]) => {
            let h = find(*id)?;
            h.to_task
                .send(message.clone())
                .map(|()| HostValue::Bool(true))
                .map_err(|_| fail("the task is over"))
        }
        ("task_recv", [HostValue::Int(id), HostValue::Int(ms)]) => {
            let h = find(*id)?;
            let rx = h.from_task.lock().unwrap_or_else(|e| e.into_inner());
            match rx.recv_timeout(millis(*ms)) {
                Ok(text) => Ok(HostValue::Str(text)),
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
                    Ok(HostValue::Null)
                }
            }
        }
        ("task_wait", [HostValue::Int(id), HostValue::Int(ms)]) => {
            let h = find(*id)?;
            let (slot, cv) = &*h.outcome;
            let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
            let (mut guard, _) = cv
                .wait_timeout_while(guard, millis(*ms), |o| o.is_none())
                .unwrap_or_else(|e| e.into_inner());
            match guard.take() {
                None => Ok(HostValue::Null),
                Some(result) => {
                    // the task is forgotten once its result is taken
                    TASKS
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .retain(|(i, _)| *i as i64 != *id);
                    match result {
                        Ok(text) => Ok(HostValue::Str(text)),
                        Err(why) => Err(fail(format!("the task failed:\n{why}"))),
                    }
                }
            }
        }
        ("task_cancel", [HostValue::Int(id)]) => {
            if let Ok(h) = find(*id) {
                h.cancel.store(true, Ordering::SeqCst);
            }
            Ok(HostValue::Bool(true))
        }
        ("task_done", [HostValue::Int(id)]) => {
            let done = match find(*id) {
                Ok(h) => h
                    .outcome
                    .0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_some(),
                Err(_) => true,
            };
            Ok(HostValue::Bool(done))
        }
        ("task_input", []) => Ok(HostValue::Str(
            inside(|i| i.input.clone()).unwrap_or_else(|_| "null".to_string()),
        )),
        ("task_emit", [HostValue::Str(message)]) => inside(|i| i.outbox.send(message.clone()))?
            .map(|()| HostValue::Bool(true))
            .map_err(|_| fail("the program no longer listens")),
        ("task_next", [HostValue::Int(ms)]) => {
            let deadline = std::time::Instant::now() + millis(*ms);
            loop {
                let step = inside(|i| {
                    if i.cancel.load(Ordering::SeqCst) {
                        return Some(Err(()));
                    }
                    match i.inbox.recv_timeout(Duration::from_millis(25)) {
                        Ok(text) => Some(Ok(text)),
                        Err(_) => None,
                    }
                })?;
                match step {
                    Some(Ok(text)) => return Ok(HostValue::Str(text)),
                    Some(Err(())) => return Ok(HostValue::Null),
                    None => {}
                }
                if std::time::Instant::now() >= deadline || stopping() {
                    return Ok(HostValue::Null);
                }
            }
        }
        ("task_cancelled", []) => Ok(HostValue::Bool(
            inside(|i| i.cancel.load(Ordering::SeqCst)).unwrap_or(false) || stopping(),
        )),
        ("task_finish", [HostValue::Str(result)]) => {
            inside(|i| i.result = Some(result.clone()))?;
            Ok(HostValue::Null)
        }
        _ => Err(wrong()),
    }
}
