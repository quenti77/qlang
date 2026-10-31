//! What a debugger needs from the interpreter (`qlang dap`).
//!
//! A hook is installed for this thread before the program runs. The interpreter then goes through
//! the tree walker (the register machine would skip statements) and asks the hook before every
//! statement whether to stop; at a stop it gives a [`Snapshot`], made only of text and numbers so
//! that it can cross to another thread, and waits for the hook to say what to do next.

use std::cell::RefCell;

/// What the hook answers before a statement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Continue,
    Stop,
    /// End the program now (the session is over).
    Abort,
}

pub trait Hook {
    /// Before the statement at `line` of `file` (the name the program was compiled with); `depth` is the
    /// number of function calls in progress.
    fn check(&mut self, file: &str, line: u32, depth: usize) -> Action;

    /// The program is stopped: `snapshot` is what it looks like. Returns once the user wants it to go on
    /// (`true`) or the session is over (`false`).
    fn stopped(&mut self, snapshot: Snapshot) -> bool;
}

/// One variable, with the number of its children (`0` for none; otherwise an index into
/// [`Snapshot::children`] plus one).
#[derive(Clone, Debug, Default)]
pub struct Var {
    pub name: String,
    pub ty: String,
    pub value: String,
    pub reference: usize,
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub name: String,
    pub reference: usize,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub scopes: Vec<Scope>,
}

/// The stopped program: the calls (innermost first) and the variables, as lists addressed by
/// `reference - 1`.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub frames: Vec<Frame>,
    pub children: Vec<Vec<Var>>,
}

thread_local! {
    static HOOK: RefCell<Option<Box<dyn Hook>>> = const { RefCell::new(None) };
}

/// Debug the next program run on this thread.
pub fn install(hook: Box<dyn Hook>) {
    HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

pub(crate) fn take() -> Option<Box<dyn Hook>> {
    HOOK.with(|h| h.borrow_mut().take())
}
