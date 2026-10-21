//! Interpreter: runs a checked [`Program`].
//!
//! Everything that reaches outside goes through the [`Host`]. Execution is
//! bounded by [`Limits`] so that a runaway program cannot hang its host.

mod externs;

use crate::ast::*;
use crate::check::Program;
use crate::datetime;
use crate::diag::Diagnostic;
use crate::host::{Host, HostError, Limits};
use crate::ids;
use crate::smallmap::{Name, SmallMap};
use crate::span::Span;
use crate::types::*;
use crate::value::*;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

pub struct RunResult {
    /// The runtime error that stopped the program, if any.
    pub error: Option<Diagnostic>,
    /// The code the program gave to `exit(code)`, if it called it.
    pub exit_code: Option<i32>,
    /// Evaluation steps used.
    pub steps: u64,
}

impl Program {
    /// Run the program. Output and input go through `host`.
    pub fn run(&self, host: &mut dyn Host, limits: Limits) -> RunResult {
        run(self, host, limits)
    }
}

pub fn run(prog: &Program, host: &mut dyn Host, limits: Limits) -> RunResult {
    let mut it = Interp::new(prog, host, limits);
    // A bug in the interpreter must not take the caller down with it (a server, an editor).
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| it.run_all()));
    let mut exit_code = None;
    let error = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(Ctrl::Error(mut d))) => {
            d.resolve(&prog.sources);
            Some(*d)
        }
        Ok(Err(Ctrl::Exit(code))) => {
            exit_code = Some(code);
            None
        }
        Ok(Err(_)) => None,
        Err(payload) => Some(crate::check::internal_error(&payload)),
    };
    RunResult {
        error,
        exit_code,
        steps: it.steps,
    }
}

/// An HTTP request for a program started with [`Program::start_app`]: the fields of the
/// prelude's `Request` struct (`headers` names in lower case).
#[derive(Clone, Debug, Default)]
pub struct AppRequest {
    pub method: String,
    /// The target as received: the path and the query (`/items?id=3`).
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// The decoded path of the target (`/items`).
    pub path: String,
    /// The decoded query parameters, in order (a repeated name: the last value wins).
    pub query: Vec<(String, String)>,
}

/// What the program's `handle` function answered: the fields of the prelude's `Response`.
#[derive(Clone, Debug)]
pub struct AppResponse {
    pub status: i64,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// A program that serves requests: its top-level code ran once (it keeps its variables, like
/// a Node server), then `handle(req: Request) -> Response` is called for every request, each
/// with a fresh step budget. The caller (a server) owns the network; the program only sees
/// values.
pub struct App<'a> {
    it: Interp<'a>,
    handler: FnId,
}

impl Program {
    /// Run the program's top-level code, then keep it ready to answer requests with its
    /// `fun handle(req: Request) -> Response`.
    pub fn start_app<'a>(
        &'a self,
        host: &'a mut dyn Host,
        limits: Limits,
    ) -> Result<App<'a>, Box<Diagnostic>> {
        let entry = self.modules.len().saturating_sub(1);
        let found = self.defs.fns.iter().position(|f| {
            f.name == "handle"
                && f.module == entry
                && f.impl_id.is_none()
                && !f.has_self
                && f.decl.is_some()
        });
        let Some(handler) = found else {
            return Err(Box::new(Diagnostic::error(
                "R070",
                "a program that serves requests needs `fun handle(req: Request) -> Response`",
                Default::default(),
            )));
        };
        let prelude = |name: &str| {
            self.defs
                .structs
                .iter()
                .position(|s| s.module == crate::check::PRELUDE_MOD && s.name == name)
        };
        let (req, resp) = (prelude("Request"), prelude("Response"));
        let f = &self.defs.fns[handler];
        let right = matches!((&f.params[..], &f.ret), ([Ty::Struct(a, _)], Ty::Struct(b, _)) if Some(*a) == req && Some(*b) == resp);
        if !right {
            return Err(Box::new(Diagnostic::error(
                "R070",
                "`handle` must be `fun handle(req: Request) -> Response`",
                f.span,
            )));
        }
        let mut it = Interp::new(self, host, limits);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| it.run_all()));
        match outcome {
            Ok(Ok(())) => Ok(App { it, handler }),
            Ok(Err(Ctrl::Exit(_))) => Err(Box::new(Diagnostic::error(
                "R090",
                "`exit` cannot be used in a program that serves requests (it would stop the server)",
                Default::default(),
            ))),
            Ok(Err(Ctrl::Error(mut d))) => {
                d.resolve(&self.sources);
                Err(d)
            }
            Ok(Err(_)) => Ok(App { it, handler }),
            Err(payload) => Err(Box::new(crate::check::internal_error(&payload))),
        }
    }
}

impl App<'_> {
    /// Answer one request. An error (a runtime error in `handle`, the step limit...) is
    /// returned for the caller to turn into a 500; the program stays usable.
    pub fn handle(&mut self, req: AppRequest) -> Result<AppResponse, Box<Diagnostic>> {
        let handler = self.handler;
        let it = &mut self.it;
        it.steps = 0;
        it.depth = 0;
        it.out_bytes = 0;
        it.frames.clear();
        it.fmt_depth = 0;
        it.fmt_nodes = 0;
        let prog = it.prog;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let value = it.request_value(&req);
            it.call_fid(handler, None, vec![value], Default::default())
        }));
        match outcome {
            Ok(Ok(v)) => it.response_of(&v).map_err(|e| match e {
                Ctrl::Error(mut d) => {
                    d.resolve(&prog.sources);
                    d
                }
                _ => Box::new(Diagnostic::error(
                    "X999",
                    "internal error",
                    Default::default(),
                )),
            }),
            Ok(Err(Ctrl::Exit(_))) => Err(Box::new(Diagnostic::error(
                "R090",
                "`exit` cannot be used in a program that serves requests (it would stop the server)",
                Default::default(),
            ))),
            Ok(Err(Ctrl::Error(mut d))) => {
                d.resolve(&prog.sources);
                Err(d)
            }
            Ok(Err(_)) => Err(Box::new(Diagnostic::error(
                "X999",
                "internal error",
                Default::default(),
            ))),
            Err(payload) => Err(Box::new(crate::check::internal_error(&payload))),
        }
    }
}

impl Program {
    /// The tests of the entry file: its top-level functions `fun test_...()` without
    /// parameters, in the order written.
    pub fn test_names(&self) -> Vec<String> {
        let entry = self.modules.len().saturating_sub(1);
        self.defs
            .fns
            .iter()
            .filter(|f| {
                f.module == entry
                    && f.impl_id.is_none()
                    && !f.has_self
                    && f.params.is_empty()
                    && f.name.starts_with("test_")
                    && f.decl.as_ref().is_some_and(|d| !d.external)
            })
            .map(|f| f.name.clone())
            .collect()
    }

    /// Runs one test on its own: the program's top-level code runs again from a clean
    /// state, then the test function, each with a fresh budget. A failed `assert`, a
    /// `panic` or any run-time error is the `Err`.
    pub fn run_test(
        &self,
        name: &str,
        host: &mut dyn Host,
        limits: Limits,
    ) -> Result<(), Box<Diagnostic>> {
        let entry = self.modules.len().saturating_sub(1);
        let Some(fid) = self.defs.fns.iter().position(|f| {
            f.name == name && f.module == entry && f.impl_id.is_none() && f.decl.is_some()
        }) else {
            return Err(Box::new(Diagnostic::error(
                "R070",
                format!("there is no test `{name}`"),
                Default::default(),
            )));
        };
        let span = self.defs.fns[fid].span;
        let mut it = Interp::new(self, host, limits);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            it.run_all()?;
            it.call_fid(fid, None, vec![], span).map(|_| ())
        }));
        match outcome {
            Ok(Ok(())) | Ok(Err(Ctrl::Return(_))) | Ok(Err(Ctrl::Exit(0))) => Ok(()),
            Ok(Err(Ctrl::Exit(code))) => Err(Box::new(Diagnostic::error(
                "R091",
                format!("the test called `exit({code})`"),
                span,
            ))),
            Ok(Err(Ctrl::Error(mut d))) => {
                d.resolve(&self.sources);
                Err(d)
            }
            Ok(Err(_)) => Ok(()),
            Err(payload) => Err(Box::new(crate::check::internal_error(&payload))),
        }
    }
}

impl Interp<'_> {
    fn request_value(&self, req: &AppRequest) -> Value {
        let body = req.body.as_deref().map_or(Value::None, Value::str);
        self.new_struct(
            self.prelude_struct("Request"),
            &[
                ("method", Value::str(&req.method)),
                ("url", Value::str(&req.url)),
                ("headers", Self::string_map(&req.headers)),
                ("body", body),
                ("path", Value::str(&req.path)),
                ("query", Self::string_map(&req.query)),
            ],
        )
    }

    /// Reads the fields of the `Response` the handler returned.
    fn response_of(&self, v: &Value) -> R<AppResponse> {
        let bad = |msg: &str| self.fail("R071", msg, Default::default());
        let Value::Struct(o) = v else {
            return bad("`handle` must return a `Response`");
        };
        let fields = o.fields.borrow();
        let status = match fields.get("status") {
            Some(Value::Int(s)) if (100..=599).contains(s) => *s,
            _ => return bad("the `status` of the `Response` must be an integer from 100 to 599"),
        };
        let body = match fields.get("body") {
            Some(Value::Str(s)) => s.to_string(),
            _ => return bad("the `body` of the `Response` must be a string"),
        };
        let headers = match fields.get("headers") {
            Some(Value::Map(m)) => m
                .borrow()
                .entries
                .iter()
                .filter_map(|(k, v)| match (k, v) {
                    (Value::Str(k), Value::Str(v)) => Some((k.to_string(), v.to_string())),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        Ok(AppResponse {
            status,
            headers,
            body,
        })
    }
}

enum Ctrl {
    Break,
    Continue,
    Return(Value),
    /// `exit(code)`: the whole program ends here, with this exit code.
    Exit(i32),
    Error(Box<Diagnostic>),
}

type R<T> = Result<T, Ctrl>;

/// Something that can be assigned to.
enum Place {
    Local(String),
    Global(ModId, String),
    Field(Rc<StructObj>, String),
    Static(StructId, String),
    Elem(Rc<RefCell<Vec<Value>>>, usize),
    MapEntry(Rc<RefCell<MapObj>>, Value),
    UserIndex(Value, Value),
    Append(Value),
}

struct Interp<'a> {
    prog: &'a Program,
    host: &'a mut dyn Host,
    limits: Limits,
    steps: u64,
    depth: usize,
    out_bytes: usize,
    envs: Vec<Env>,
    statics: HashMap<(StructId, String), Value>,
    frames: Vec<(Name, Span)>,
    /// The call expression being evaluated (for `sum()` on empty float arrays).
    cur_call: NodeId,
    /// Nesting and size of the value being converted to text (cycles, huge values).
    fmt_depth: usize,
    fmt_nodes: usize,
    /// The last `ulid` made by `ulid.new()`, to keep the next ones in order.
    last_ulid: u128,
}

/// Deeper values print as `...` (this also stops cyclic structures).
const MAX_FMT_DEPTH: usize = 64;

impl<'a> Interp<'a> {
    fn new(prog: &'a Program, host: &'a mut dyn Host, limits: Limits) -> Interp<'a> {
        Interp {
            prog,
            host,
            limits,
            steps: 0,
            depth: 0,
            out_bytes: 0,
            envs: Vec::new(),
            statics: HashMap::new(),
            frames: Vec::new(),
            cur_call: 0,
            fmt_depth: 0,
            fmt_nodes: 0,
            last_ulid: 0,
        }
    }

    // ------------------------------------------------------------------ errors

    fn error(&self, code: &str, msg: impl Into<String>, span: Span) -> Ctrl {
        // An error raised inside the prelude (a failed `assert_eq`, an `unwrap` of an
        // error) is shown at the call in the user's code, not at a line of the prelude.
        let prelude_file = self.prog.modules[crate::check::PRELUDE_MOD].file;
        let mut span = span;
        let mut outer = self.frames.len();
        let mut inside: Option<String> = None;
        if span.file == prelude_file
            && let Some(i) = self
                .frames
                .iter()
                .rposition(|(_, call_span)| call_span.file != prelude_file)
        {
            span = self.frames[i].1;
            inside = Some(self.frames[i].0.as_str().to_string());
            outer = i;
        }
        let mut d = Diagnostic::error(code, msg, span);
        if let Some(name) = inside {
            d = d.with_note(
                format!("raised inside `{name}`, of the standard library"),
                None,
            );
        }
        // innermost call first; identical consecutive calls (recursion) are merged
        let mut groups: Vec<(&str, Span, usize)> = Vec::new();
        for (name, call_span) in self.frames[..outer].iter().rev() {
            match groups.last_mut() {
                Some((n, s, count)) if *n == name.as_str() && *s == *call_span => *count += 1,
                _ => groups.push((name.as_str(), *call_span, 1)),
            }
        }
        for (name, call_span, count) in groups.into_iter().take(8) {
            let times = if count > 1 {
                format!(" ({count} times)")
            } else {
                String::new()
            };
            d = d.with_note(format!("called from `{name}`{times}"), Some(call_span));
        }
        Ctrl::Error(Box::new(d))
    }

    fn fail<T>(&self, code: &str, msg: impl Into<String>, span: Span) -> R<T> {
        Err(self.error(code, msg, span))
    }

    fn tick(&mut self, span: Span) -> R<()> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            return self.fail(
                "R900",
                "the program ran for too long (step limit reached: infinite loop?)",
                span,
            );
        }
        Ok(())
    }

    /// Charge the work of a built-in operation to the step budget: about one step
    /// per 4 elements visited. Without it, `xs.contains(v)` on a big array would be free.
    fn charge(&mut self, elements: usize, span: Span) -> R<()> {
        self.steps = self.steps.saturating_add((elements / 4) as u64);
        if self.steps > self.limits.max_steps {
            return self.fail(
                "R900",
                "the program ran for too long (step limit reached: infinite loop?)",
                span,
            );
        }
        Ok(())
    }

    /// Same for work proportional to a number of bytes (copying or scanning a string).
    fn charge_bytes(&mut self, bytes: usize, span: Span) -> R<()> {
        self.charge(bytes / 8, span)
    }

    /// Refuse arrays and strings that grow beyond the configured size.
    fn guard_alloc(&self, len: usize, span: Span) -> R<()> {
        if len > self.limits.max_alloc {
            return self.fail(
                "R903",
                "a value became too large (memory limit reached)",
                span,
            );
        }
        Ok(())
    }

    fn emit(&mut self, text: &str, span: Span) -> R<()> {
        self.out_bytes += text.len();
        if self.out_bytes > self.limits.max_output {
            return self.fail(
                "R902",
                "the program printed too much (output limit reached)",
                span,
            );
        }
        self.host.print(text);
        Ok(())
    }

    // --------------------------------------------------------------- program

    fn run_all(&mut self) -> R<()> {
        let prog = self.prog;
        let n = prog.modules.len();
        for _ in 0..n {
            self.envs.push(Scope::new(None));
        }
        for m in 0..n {
            self.init_statics(m)?;
            let ast = prog.modules[m].ast.clone();
            let env = self.envs[m].clone();
            for item in &ast.items {
                if let Item::Stmt(s) = item {
                    match self.exec_stmt(s, &env) {
                        Ok(_) => {}
                        Err(Ctrl::Return(_)) => break,
                        Err(Ctrl::Break) | Err(Ctrl::Continue) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        Ok(())
    }

    fn init_statics(&mut self, m: ModId) -> R<()> {
        let prog = self.prog;
        for (sid, sd) in prog.defs.structs.iter().enumerate() {
            if sd.module != m {
                continue;
            }
            for f in &sd.fields {
                if let (true, Some(d)) = (f.is_static, &f.default) {
                    let env = Scope::new(Some(self.envs[m].clone()));
                    let v = self.eval(d, &env)?;
                    self.statics.insert((sid, f.name.clone()), v);
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ statements

    fn exec_stmts(&mut self, stmts: &[Stmt], env: &Env) -> R<Value> {
        let mut last = Value::Unit;
        for s in stmts {
            last = self.exec_stmt(s, env)?;
        }
        Ok(last)
    }

    fn exec_block(&mut self, b: &Block, env: &Env) -> R<Value> {
        // a block that declares nothing has no variables of its own: run it in the
        // enclosing scope instead of making a new one for every pass through a loop
        if !b
            .stmts
            .iter()
            .any(|s| matches!(s.kind, StmtKind::Let { .. }))
        {
            return self.exec_stmts(&b.stmts, env);
        }
        let scope = Scope::new(Some(env.clone()));
        self.exec_stmts(&b.stmts, &scope)
    }

    fn exec_stmt(&mut self, s: &Stmt, env: &Env) -> R<Value> {
        self.tick(s.span)?;
        match &s.kind {
            StmtKind::Expr(e) => self.eval(e, env),
            StmtKind::Let { name, init, .. } => {
                let v = self.eval(init, env)?;
                env.define(&name.name, v);
                Ok(Value::Unit)
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Unit,
                };
                Err(Ctrl::Return(v))
            }
            StmtKind::Break => Err(Ctrl::Break),
            StmtKind::Continue => Err(Ctrl::Continue),
            StmtKind::While { cond, body } => self.exec_while(s, cond, body, env),
            StmtKind::For {
                var,
                var2,
                iter,
                step,
                body,
            } => self.exec_for(s, var, var2, iter, step, body, env),
        }
    }

    #[inline(never)]
    fn exec_while(&mut self, s: &Stmt, cond: &Expr, body: &Block, env: &Env) -> R<Value> {
        loop {
            if !self.eval_bool(cond, env)? {
                break;
            }
            match self.exec_block(body, env) {
                Ok(_) | Err(Ctrl::Continue) => {}
                Err(Ctrl::Break) => break,
                Err(e) => return Err(e),
            }
            self.tick(s.span)?;
        }
        Ok(Value::Unit)
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn exec_for(
        &mut self,
        s: &Stmt,
        var: &Ident,
        var2: &Option<Ident>,
        iter: &Expr,
        step: &Option<Expr>,
        body: &Block,
        env: &Env,
    ) -> R<Value> {
        let it = self.eval(iter, env)?;
        let step_v = match step {
            Some(st) => match self.eval(st, env)? {
                Value::Int(i) => Some(i),
                _ => return self.fail("R030", "`step` must be an integer", st.span),
            },
            None => None,
        };
        let run_body = |this: &mut Self, v: Value, v2: Option<Value>| -> R<bool> {
            let scope = Scope::new(Some(env.clone()));
            scope.define(&var.name, v);
            if let (Some(name), Some(second)) = (var2, v2) {
                scope.define(&name.name, second);
            }
            match this.exec_stmts(&body.stmts, &scope) {
                Ok(_) | Err(Ctrl::Continue) => Ok(true),
                Err(Ctrl::Break) => Ok(false),
                Err(e) => Err(e),
            }
        };
        match it {
            Value::Range(start, end, incl) => {
                let step = step_v.unwrap_or(1);
                if step == 0 {
                    return self.fail("R031", "`step` cannot be 0", s.span);
                }
                let mut i = start;
                loop {
                    let in_range = if step > 0 {
                        if incl { i <= end } else { i < end }
                    } else if incl {
                        i >= end
                    } else {
                        i > end
                    };
                    if !in_range {
                        break;
                    }
                    if !run_body(self, Value::Int(i), None)? {
                        break;
                    }
                    self.tick(s.span)?;
                    i = match i.checked_add(step) {
                        Some(n) => n,
                        None => break,
                    };
                }
            }
            Value::Array(a) => {
                let mut i = 0;
                loop {
                    let item = a.borrow().get(i).cloned();
                    let Some(v) = item else { break };
                    let ok = if var2.is_some() {
                        run_body(self, Value::Int(i as i64), Some(v))?
                    } else {
                        run_body(self, v, None)?
                    };
                    if !ok {
                        break;
                    }
                    self.tick(s.span)?;
                    i += 1;
                }
            }
            Value::Str(st) => {
                for (i, ch) in st.chars().enumerate() {
                    let c = Value::str(&ch.to_string());
                    let ok = if var2.is_some() {
                        run_body(self, Value::Int(i as i64), Some(c))?
                    } else {
                        run_body(self, c, None)?
                    };
                    if !ok {
                        break;
                    }
                    self.tick(s.span)?;
                }
            }
            Value::Map(m) => {
                // a snapshot: the map may be changed by the loop body
                self.charge(m.borrow().len(), s.span)?;
                let entries: Vec<(Value, Value)> = m.borrow().entries.clone();
                for (k, v) in entries {
                    if !run_body(self, k, Some(v))? {
                        break;
                    }
                    self.tick(s.span)?;
                }
            }
            _ => return self.fail("R032", "this value cannot be looped over", iter.span),
        }
        Ok(Value::Unit)
    }

    fn eval_bool(&mut self, e: &Expr, env: &Env) -> R<bool> {
        match self.eval(e, env)? {
            Value::Bool(b) => Ok(b),
            _ => self.fail("R001", "a `bool` was expected here", e.span),
        }
    }

    // ----------------------------------------------------------- expressions

    fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        self.tick(e.span)?;
        let prog = self.prog;
        match &e.kind {
            ExprKind::Int(v) => Ok(Value::Int(*v)),
            ExprKind::Float(v) => Ok(Value::Float(*v)),
            ExprKind::Bool(v) => Ok(Value::Bool(*v)),
            ExprKind::None => Ok(Value::None),
            ExprKind::Str(parts) => self.eval_str(e, parts, env),
            ExprKind::Ident(name, _) => match prog.res.paths.get(&e.id) {
                Some(PathRes::Global(m, n)) => match self.envs[*m].get(n) {
                    Some(v) => Ok(v),
                    None => self.fail(
                        "R002",
                        format!("`{n}` is used before it has a value"),
                        e.span,
                    ),
                },
                Some(PathRes::Fn(fid)) => Ok(self.fn_value(*fid)),
                _ => match env.get(name) {
                    Some(v) => Ok(v),
                    None => self.fail(
                        "R002",
                        format!("`{name}` is used before it has a value"),
                        e.span,
                    ),
                },
            },
            ExprKind::SelfVal => match env.get("self") {
                Some(v) => Ok(v),
                None => self.fail("R002", "`self` is not available here", e.span),
            },
            ExprKind::Super => self.fail("R003", "`super` can only be used for calls", e.span),
            ExprKind::Array(items) => self.eval_array(items, env),
            ExprKind::Map(items) => self.eval_map(e, items, env),
            ExprKind::StructLit(lit) => self.eval_struct_lit(e, lit, env),
            ExprKind::Unary(op, inner) => self.eval_unary(e, op, inner, env),
            ExprKind::Binary(op, l, r) => match op {
                BinOp::And => {
                    if !self.eval_bool(l, env)? {
                        return Ok(Value::Bool(false));
                    }
                    Ok(Value::Bool(self.eval_bool(r, env)?))
                }
                BinOp::Or => {
                    if self.eval_bool(l, env)? {
                        return Ok(Value::Bool(true));
                    }
                    Ok(Value::Bool(self.eval_bool(r, env)?))
                }
                _ => {
                    let lv = self.eval(l, env)?;
                    let rv = self.eval(r, env)?;
                    self.binary(*op, lv, rv, e.span)
                }
            },
            ExprKind::Assign { op, target, value } => self.eval_assign(e, *op, target, value, env),
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => self.eval_range(e, start, end, *inclusive, env),
            ExprKind::Try(inner) => self.eval_try(e, inner, env),
            ExprKind::Cast(inner, _) => self.eval_cast(e, inner, env),
            ExprKind::Call { callee, args } => self.eval_call(e, callee, args, env),
            ExprKind::Index(base, idx) => self.eval_index(e, base, idx, env),
            ExprKind::Append(_) => self.fail("R003", "`x[]` can only be assigned to", e.span),
            ExprKind::Field(obj, name, _) => self.eval_field(e, obj, name, env),
            ExprKind::If {
                branches,
                else_block,
            } => {
                for (cond, block) in branches {
                    if self.eval_bool(cond, env)? {
                        return self.exec_block(block, env);
                    }
                }
                match else_block {
                    Some(b) => self.exec_block(b, env),
                    None => Ok(Value::Unit),
                }
            }
            ExprKind::Match {
                scrutinee,
                cases,
                else_block,
            } => self.eval_match(e, scrutinee, cases, else_block, env),
            ExprKind::Lambda(f) => Ok(Value::Func(Rc::new(FuncVal {
                decl: f.clone(),
                env: env.clone(),
                name: "<function>".into(),
            }))),
        }
    }

    #[inline(never)]
    fn eval_str(&mut self, e: &Expr, parts: &[StrPart], env: &Env) -> R<Value> {
        let mut out = String::new();
        for p in parts {
            match p {
                StrPart::Lit(s) => out.push_str(s),
                StrPart::Expr(x) => {
                    let v = self.eval(x, env)?;
                    out.push_str(&self.stringify(&v, x.span)?);
                }
            }
        }
        self.guard_alloc(out.len(), e.span)?;
        self.charge_bytes(out.len(), e.span)?;
        Ok(Value::str(&out))
    }

    #[inline(never)]
    fn eval_array(&mut self, items: &[Expr], env: &Env) -> R<Value> {
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            out.push(self.eval(it, env)?);
        }
        Ok(Value::array(out))
    }

    #[inline(never)]
    fn eval_map(&mut self, e: &Expr, items: &[(Expr, Expr)], env: &Env) -> R<Value> {
        let mut m = MapObj::default();
        for (k, v) in items {
            let kv = self.eval(k, env)?;
            let vv = self.eval(v, env)?;
            self.guard_alloc(m.len() + 1, e.span)?;
            if !m.insert(kv, vv) {
                return self.fail("R023", "this value cannot be used as a map key", k.span);
            }
        }
        Ok(Value::Map(Rc::new(RefCell::new(m))))
    }

    #[inline(never)]
    fn eval_unary(&mut self, e: &Expr, op: &UnOp, inner: &Expr, env: &Env) -> R<Value> {
        let prog = self.prog;

        let v = self.eval(inner, env)?;
        match (op, &v) {
            (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
            (UnOp::Neg, Value::Int(i)) => match i.checked_neg() {
                Some(n) => Ok(Value::Int(n)),
                None => self.fail("R010", "integer overflow", e.span),
            },
            (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
            (UnOp::Neg, _) => match self.find_method(
                &v,
                Lang::Neg.method_name(),
                Some(prog.defs.lang(Lang::Neg)),
                None,
            ) {
                Some(fid) => self.call_fid(fid, Some(v), vec![], e.span),
                None => self.fail("R004", "this value cannot be negated", e.span),
            },
            _ => self.fail("R001", "unexpected operand", e.span),
        }
    }

    #[inline(never)]
    fn eval_range(
        &mut self,
        e: &Expr,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        env: &Env,
    ) -> R<Value> {
        let (a, b) = (self.eval(start, env)?, self.eval(end, env)?);
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Range(a, b, inclusive)),
            _ => self.fail("R001", "a range needs integer bounds", e.span),
        }
    }

    #[inline(never)]
    fn eval_try(&mut self, e: &Expr, inner: &Expr, env: &Env) -> R<Value> {
        let prog = self.prog;

        let v = self.eval(inner, env)?;
        if prog.res.result_tries.contains(&e.id) {
            // `Ok(x)` gives x; `Err(..)` leaves the function with that same error
            return match &v {
                Value::Enum(_, vi, Some(_)) if self.is_err_variant(&v, *vi) => {
                    Err(Ctrl::Return(v.clone()))
                }
                Value::Enum(_, _, Some(data)) if data.len() == 1 => Ok(data[0].clone()),
                _ => self.fail("R001", "`?` expects a `Result`", e.span),
            };
        }
        match v {
            Value::None => Err(Ctrl::Return(Value::None)),
            other => Ok(other),
        }
    }

    #[inline(never)]
    fn eval_cast(&mut self, e: &Expr, inner: &Expr, env: &Env) -> R<Value> {
        let prog = self.prog;

        let v = self.eval(inner, env)?;
        let target = prog.res.casts.get(&e.id).cloned().unwrap_or(Ty::Error);
        self.convert(v, &target, e.span)
    }

    #[inline(never)]
    fn eval_index(&mut self, e: &Expr, base: &Expr, idx: &Expr, env: &Env) -> R<Value> {
        let b = self.eval(base, env)?;
        let i = self.eval(idx, env)?;
        self.index_get(&b, &i, e.span)
    }

    #[inline(never)]
    fn eval_match(
        &mut self,
        e: &Expr,
        scrutinee: &Expr,
        cases: &[MatchCase],
        else_block: &Option<Block>,
        env: &Env,
    ) -> R<Value> {
        let v = self.eval(scrutinee, env)?;
        for (i, case) in cases.iter().enumerate() {
            let scope = Scope::new(Some(env.clone()));
            if !self.match_pattern(&case.pattern, &v, e.id, i, &scope)? {
                continue;
            }
            if let Some(g) = &case.guard
                && !self.eval_bool(g, &scope)?
            {
                continue;
            }
            return self.exec_stmts(&case.body.stmts, &scope);
        }
        match else_block {
            Some(b) => self.exec_block(b, env),
            None => Ok(Value::Unit),
        }
    }

    fn fn_value(&self, fid: FnId) -> Value {
        let def = &self.prog.defs.fns[fid];
        Value::Func(Rc::new(FuncVal {
            decl: def.decl.clone().expect("function without declaration"),
            env: self.envs[def.module].clone(),
            name: def.name.clone(),
        }))
    }

    fn eval_struct_lit(&mut self, e: &Expr, lit: &StructLit, env: &Env) -> R<Value> {
        let prog = self.prog;
        let sid = prog.res.struct_lits[&e.id];
        let mut fields = SmallMap::default();
        if let Some(b) = &lit.base {
            match self.eval(b, env)? {
                Value::Struct(o) => {
                    for (k, v) in o.fields.borrow().iter() {
                        fields.insert(k, v.clone());
                    }
                }
                _ => return self.fail("R001", "`..base` must be a struct value", b.span),
            }
        }
        for item in &lit.fields {
            let v = self.eval(&item.value, env)?;
            fields.insert(&item.name.name, v);
        }
        for (asid, _) in prog.defs.ancestry(sid, &[]) {
            let sd = &prog.defs.structs[asid];
            for f in &sd.fields {
                if f.is_static || fields.contains_key(&f.name) {
                    continue;
                }
                if let Some(d) = &f.default {
                    let scope = Scope::new(Some(self.envs[sd.module].clone()));
                    let v = self.eval(d, &scope)?;
                    fields.insert(&f.name, v);
                }
            }
        }
        Ok(Value::Struct(Rc::new(StructObj {
            def: sid,
            fields: RefCell::new(fields),
        })))
    }

    fn eval_field(&mut self, e: &Expr, obj: &Expr, name: &Ident, env: &Env) -> R<Value> {
        let prog = self.prog;
        match prog.res.paths.get(&e.id) {
            Some(PathRes::EnumVariant(eid, vi)) => return Ok(Value::Enum(*eid, *vi, None)),
            Some(PathRes::StaticField(sid, n)) => {
                return match self.statics.get(&(*sid, n.clone())) {
                    Some(v) => Ok(v.clone()),
                    None => self.fail(
                        "R002",
                        format!("static field `{n}` has no value yet"),
                        e.span,
                    ),
                };
            }
            Some(PathRes::BuiltinStatic(ty, name)) => {
                return Ok(match (*ty, *name) {
                    ("float", "PI") => Value::Float(std::f64::consts::PI),
                    ("float", "E") => Value::Float(std::f64::consts::E),
                    ("float", "INFINITY") => Value::Float(f64::INFINITY),
                    ("float", "NAN") => Value::Float(f64::NAN),
                    ("int", "MAX") => Value::Int(i64::MAX),
                    ("int", "MIN") => Value::Int(i64::MIN),
                    ("uuid", "NIL") => Value::Uuid(0),
                    _ => Value::Unit,
                });
            }
            Some(PathRes::ClassName(n)) => return Ok(Value::str(n)),
            Some(PathRes::Fn(fid)) => return Ok(self.fn_value(*fid)),
            Some(PathRes::Global(m, n)) => {
                return match self.envs[*m].get(n) {
                    Some(v) => Ok(v),
                    None => self.fail(
                        "R002",
                        format!("`{n}` is used before it has a value"),
                        e.span,
                    ),
                };
            }
            _ => {}
        }
        match self.eval(obj, env)? {
            Value::Struct(o) => {
                let v = o.fields.borrow().get(&name.name).cloned();
                match v {
                    Some(v) => Ok(v),
                    None => self.fail(
                        "R005",
                        format!("this value has no field `{}`", name.name),
                        name.span,
                    ),
                }
            }
            Value::None => self.fail(
                "R006",
                format!("cannot read `.{}`: the value is `none`", name.name),
                e.span,
            ),
            _ => self.fail(
                "R005",
                format!("this value has no field `{}`", name.name),
                name.span,
            ),
        }
    }

    // ----------------------------------------------------------------- calls

    /// Evaluates arguments in the order written, then places each value in the parameter
    /// slot it fills (identity unless the call mixed in named arguments: see `arg_order`).
    fn eval_args(&mut self, node: NodeId, args: &[CallArg], env: &Env) -> R<Vec<Value>> {
        if let Some(order) = self.prog.res.arg_order.get(&node).cloned() {
            let mut out: Vec<Option<Value>> = (0..args.len()).map(|_| None).collect();
            for (a, slot) in args.iter().zip(order.iter()) {
                out[*slot] = Some(self.eval(&a.value, env)?);
            }
            return Ok(out
                .into_iter()
                .map(|v| v.expect("every parameter slot filled by the checker"))
                .collect());
        }
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(&a.value, env)?);
        }
        Ok(out)
    }

    fn eval_call(&mut self, e: &Expr, callee: &Expr, args: &[CallArg], env: &Env) -> R<Value> {
        let prog = self.prog;
        match &callee.kind {
            ExprKind::Ident(..) | ExprKind::Field(..) => {
                match prog.res.paths.get(&callee.id) {
                    Some(PathRes::Fn(fid)) => {
                        let argv = self.eval_args(e.id, args, env)?;
                        return self.call_fid(*fid, None, argv, e.span);
                    }
                    Some(PathRes::BuiltinFn(name)) => {
                        let argv = self.eval_args(e.id, args, env)?;
                        return self.builtin_fn(name, argv, e.span);
                    }
                    // `Shape.Circle(2.0)`: a variant that carries data
                    Some(PathRes::EnumVariant(eid, vi)) => {
                        let argv = self.eval_args(e.id, args, env)?;
                        return Ok(Value::Enum(*eid, *vi, Some(argv.into())));
                    }
                    Some(PathRes::BuiltinStatic(ty, name)) => {
                        let argv = self.eval_args(e.id, args, env)?;
                        return self.static_call(ty, name, argv, e.span);
                    }
                    _ => {}
                }
                if let ExprKind::Field(obj, name, _) = &callee.kind {
                    if let Some(fid) = prog.res.super_calls.get(&callee.id) {
                        let this = env.get("self");
                        let argv = self.eval_args(e.id, args, env)?;
                        return self.call_fid(*fid, this, argv, e.span);
                    }
                    // a module member that is a variable holding a function, or a method call
                    if !matches!(
                        prog.res.paths.get(&obj.id),
                        Some(PathRes::Module(_)) | Some(PathRes::Type(_))
                    ) {
                        let recv = self.eval(obj, env)?;
                        let argv = self.eval_args(e.id, args, env)?;
                        self.cur_call = e.id;
                        return self.call_method(recv, name, argv, e.span);
                    }
                }
            }
            _ => {}
        }
        let f = self.eval(callee, env)?;
        let argv = self.eval_args(e.id, args, env)?;
        self.call_value(&f, argv, e.span)
    }

    fn call_value(&mut self, f: &Value, args: Vec<Value>, span: Span) -> R<Value> {
        match f {
            Value::Func(fv) => {
                let fv = fv.clone();
                self.call_function(&fv.decl, &fv.env, None, args, &fv.name, span)
            }
            _ => self.fail("R007", "this value is not a function", span),
        }
    }

    fn call_fid(
        &mut self,
        fid: FnId,
        this: Option<Value>,
        args: Vec<Value>,
        span: Span,
    ) -> R<Value> {
        let prog = self.prog;
        let def = &prog.defs.fns[fid];
        let decl = def.decl.clone().expect("function without body");
        let env = self.envs[def.module].clone();
        self.call_function(&decl, &env, this, args, &def.name, span)
    }

    fn call_function(
        &mut self,
        decl: &Rc<FunDecl>,
        parent: &Env,
        this: Option<Value>,
        args: Vec<Value>,
        name: &str,
        span: Span,
    ) -> R<Value> {
        if decl.external {
            self.frames.push((Name::new(name), span));
            let r = self.call_extern(decl, args, span);
            self.frames.pop();
            return r;
        }
        if self.depth >= self.limits.max_depth {
            return self.fail("R901", "too many nested calls (infinite recursion?)", span);
        }
        self.depth += 1;
        self.frames.push((Name::new(name), span));
        let scope = Scope::new(Some(parent.clone()));
        if let Some(s) = this {
            scope.define("self", s);
        }
        for (p, a) in decl.params.iter().zip(args) {
            scope.define(&p.name.name, a);
        }
        let r = match &decl.body {
            Some(b) => self.exec_stmts(&b.stmts, &scope),
            None => Ok(Value::Unit),
        };
        self.frames.pop();
        self.depth -= 1;
        match r {
            Ok(v) => Ok(v),
            Err(Ctrl::Return(v)) => Ok(v),
            Err(Ctrl::Break) | Err(Ctrl::Continue) => Ok(Value::Unit),
            Err(e) => Err(e),
        }
    }

    fn call_method(&mut self, recv: Value, name: &Ident, args: Vec<Value>, span: Span) -> R<Value> {
        if let Value::None = recv {
            return self.fail(
                "R006",
                format!("cannot call `.{}()`: the value is `none`", name.name),
                span,
            );
        }
        if self.prog.res.class_calls.contains(&self.cur_call) {
            return Ok(Value::str(self.class_name_of(&recv)));
        }
        // A call made through a trait (a generic bound or a trait-typed value) goes to the
        // user's impl; a call on a concrete built-in type goes to the built-in method.
        // The checker decided which, so that both sides agree.
        let via_trait = self.prog.res.trait_calls.contains(&self.cur_call);
        if !via_trait
            && !matches!(recv, Value::Struct(_) | Value::Enum(..))
            && let Some(v) = self.builtin_method(&recv, &name.name, &args, span)?
        {
            return Ok(v);
        }
        if let Some(fid) = self.find_method(&recv, &name.name, None, args.first()) {
            return self.call_fid(fid, Some(recv), args, span);
        }
        // a field holding a function
        if let Value::Struct(o) = &recv {
            let f = o.fields.borrow().get(&name.name).cloned();
            if let Some(f) = f {
                return self.call_value(&f, args, span);
            }
        }
        self.fail(
            "R008",
            format!("this value has no method `{}`", name.name),
            span,
        )
    }

    /// The name of a value's concrete struct or enum, for `.class()`.
    fn class_name_of(&self, v: &Value) -> &str {
        match v {
            Value::Struct(o) => &self.prog.defs.structs[o.def].name,
            Value::Enum(eid, ..) => &self.prog.defs.enums[*eid].name,
            _ => "?",
        }
    }

    // ---------------------------------------------------------- method lookup

    fn value_matches_ty(&self, v: &Value, t: &Ty) -> bool {
        let defs = &self.prog.defs;
        match (v, t) {
            (_, Ty::Error | Ty::Param(_) | Ty::Infer(_)) => true,
            (Value::Int(_), Ty::Int)
            | (Value::Float(_), Ty::Float)
            | (Value::Bool(_), Ty::Bool) => true,
            (Value::Str(_), Ty::Str) | (Value::Range(..), Ty::Range) => true,
            (Value::Uuid(_), Ty::Uuid)
            | (Value::Ulid(_), Ty::Ulid)
            | (Value::DateTime(_), Ty::DateTime) => true,
            (Value::None, Ty::NoneT | Ty::Nullable(_)) => true,
            (_, Ty::Nullable(inner)) => self.value_matches_ty(v, inner),
            (Value::Array(_), Ty::Array(_)) => true,
            (Value::Map(_), Ty::Map(..)) => true,
            (Value::Struct(o), Ty::Struct(sid, _)) => defs.is_subclass(o.def, *sid),
            (Value::Struct(o), Ty::Trait(tid, _)) => {
                let classes: Vec<StructId> = defs
                    .ancestry(o.def, &[])
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect();
                defs.impls.iter().any(|imp| {
                    matches!(&imp.trait_, Some((t2, _)) if t2 == tid)
                        && matches!(&imp.target, Ty::Struct(s, _) if classes.contains(s))
                })
            }
            (Value::Enum(e, ..), Ty::Enum(x, _)) => e == x,
            (Value::Func(_), Ty::Fn(..)) => true,
            _ => false,
        }
    }

    /// The impl method `name` for a value, optionally restricted to a trait and
    /// to the impl whose first parameter accepts `arg`.
    fn find_method(
        &self,
        recv: &Value,
        name: &str,
        tid: Option<TraitId>,
        arg: Option<&Value>,
    ) -> Option<FnId> {
        let defs = &self.prog.defs;
        let classes: Vec<Option<StructId>> = match recv {
            Value::Struct(o) => defs
                .ancestry(o.def, &[])
                .into_iter()
                .map(|(s, _)| Some(s))
                .collect(),
            _ => vec![None],
        };
        for cls in classes {
            for pass in 0..2 {
                for imp in &defs.impls {
                    if (pass == 0) != imp.trait_.is_none() {
                        continue;
                    }
                    if let Some(t) = tid
                        && !matches!(&imp.trait_, Some((x, _)) if *x == t)
                    {
                        continue;
                    }
                    let target_ok = match (&imp.target, cls) {
                        (Ty::Struct(s, _), Some(c)) => *s == c,
                        (Ty::Struct(..), None) => false,
                        (t, None) => self.value_matches_ty(recv, t),
                        _ => false,
                    };
                    if !target_ok {
                        continue;
                    }
                    let Some((_, fid)) = imp.methods.iter().find(|(n, _)| n == name) else {
                        continue;
                    };
                    if let Some(a) = arg
                        && let Some(p0) = defs.fns[*fid].params.first()
                        && !self.value_matches_ty(a, p0)
                    {
                        continue;
                    }
                    return Some(*fid);
                }
            }
        }
        None
    }

    /// The `As<target>` impl for a value.
    fn find_cast_impl(&self, recv: &Value, target: &Ty) -> Option<FnId> {
        let defs = &self.prog.defs;
        let tid = defs.lang(Lang::As);
        let classes: Vec<Option<StructId>> = match recv {
            Value::Struct(o) => defs
                .ancestry(o.def, &[])
                .into_iter()
                .map(|(s, _)| Some(s))
                .collect(),
            _ => vec![None],
        };
        for cls in classes {
            for imp in &defs.impls {
                let Some((t, a)) = &imp.trait_ else { continue };
                if *t != tid || a.first() != Some(target) {
                    continue;
                }
                let ok = match (&imp.target, cls) {
                    (Ty::Struct(s, _), Some(c)) => *s == c,
                    (Ty::Struct(..), None) => false,
                    (t, None) => self.value_matches_ty(recv, t),
                    _ => false,
                };
                if ok {
                    return imp
                        .methods
                        .iter()
                        .find(|(n, _)| n == "convert")
                        .map(|(_, f)| *f);
                }
            }
        }
        None
    }

    // ----------------------------------------------------------- conversions

    fn convert(&mut self, v: Value, target: &Ty, span: Span) -> R<Value> {
        match (&v, target) {
            (Value::Int(i), Ty::Float) => return Ok(Value::Float(*i as f64)),
            (Value::Float(f), Ty::Int) => {
                if !f.is_finite() || *f >= 9.3e18 || *f <= -9.3e18 {
                    return self.fail("R013", format!("cannot convert {f} to an integer"), span);
                }
                return Ok(Value::Int(f.trunc() as i64));
            }
            (_, Ty::Str) => {
                let s = self.stringify(&v, span)?;
                return Ok(Value::str(&s));
            }
            _ => {}
        }
        if self.value_matches_ty(&v, target) {
            return Ok(v);
        }
        match self.find_cast_impl(&v, target) {
            Some(fid) => self.call_fid(fid, Some(v), vec![], span),
            None => self.fail("R009", "this conversion is not available", span),
        }
    }

    fn stringify(&mut self, v: &Value, span: Span) -> R<String> {
        if self.fmt_depth == 0 {
            self.fmt_nodes = 0;
        }
        if self.fmt_depth >= MAX_FMT_DEPTH {
            return Ok("...".to_string());
        }
        self.fmt_nodes += 1;
        if self.fmt_nodes > self.limits.max_alloc {
            return self.fail(
                "R903",
                "this value is too large to print (memory limit reached)",
                span,
            );
        }
        self.tick(span)?;
        self.fmt_depth += 1;
        let r = self.stringify_inner(v, span);
        self.fmt_depth -= 1;
        r
    }

    fn stringify_inner(&mut self, v: &Value, span: Span) -> R<String> {
        Ok(match v {
            Value::Str(s) => s.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => fmt_float(*f),
            Value::Bool(b) => b.to_string(),
            Value::Uuid(u) => ids::format_uuid(*u),
            Value::Ulid(u) => ids::format_ulid(*u),
            Value::DateTime(t) => datetime::to_iso(*t),
            Value::None => "none".to_string(),
            Value::Unit => String::new(),
            Value::Array(a) => {
                let items: Vec<Value> = a.borrow().clone();
                let mut parts = Vec::with_capacity(items.len());
                for it in &items {
                    parts.push(self.repr(it, span)?);
                }
                format!("[{}]", parts.join(", "))
            }
            Value::Map(m) => {
                let entries: Vec<(Value, Value)> = m.borrow().entries.clone();
                let mut parts = Vec::with_capacity(entries.len());
                for (k, v) in &entries {
                    parts.push(format!("{}: {}", self.repr(k, span)?, self.repr(v, span)?));
                }
                format!("{{{}}}", parts.join(", "))
            }
            Value::Enum(e, vi, data) => {
                let d = &self.prog.defs.enums[*e];
                let name = format!("{}.{}", d.name, d.variants[*vi].name);
                match data {
                    None => name,
                    Some(items) => {
                        let items = items.clone();
                        let mut parts = Vec::with_capacity(items.len());
                        for it in items.iter() {
                            parts.push(self.repr(it, span)?);
                        }
                        format!("{name}({})", parts.join(", "))
                    }
                }
            }
            Value::Range(a, b, incl) => format!("{a}{}{b}", if *incl { "..=" } else { ".." }),
            Value::Func(_) => "<function>".to_string(),
            Value::Struct(o) => match self.find_cast_impl(v, &Ty::Str) {
                Some(fid) => match self.call_fid(fid, Some(v.clone()), vec![], span)? {
                    Value::Str(s) => s.to_string(),
                    _ => return self.fail("R001", "`convert` must return a string", span),
                },
                None => {
                    let defs = &self.prog.defs;
                    let mut names: Vec<String> = Vec::new();
                    for (sid, _) in defs.ancestry(o.def, &[]).into_iter().rev() {
                        for f in &defs.structs[sid].fields {
                            if !f.is_static {
                                names.push(f.name.clone());
                            }
                        }
                    }
                    let mut parts = Vec::new();
                    for n in names {
                        let fv = o.fields.borrow().get(&n).cloned();
                        if let Some(fv) = fv {
                            parts.push(format!("{n}: {}", self.repr(&fv, span)?));
                        }
                    }
                    let name = &self.prog.defs.structs[o.def].name;
                    if parts.is_empty() {
                        name.clone()
                    } else {
                        format!("{name} {{ {} }}", parts.join(", "))
                    }
                }
            },
        })
    }

    /// Like `stringify`, but strings are quoted (used inside arrays and structs).
    fn repr(&mut self, v: &Value, span: Span) -> R<String> {
        if let Value::Str(s) = v {
            let esc = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\t', "\\t");
            return Ok(format!("\"{esc}\""));
        }
        self.stringify(v, span)
    }

    // -------------------------------------------------------------- operators

    fn binary(&mut self, op: BinOp, l: Value, r: Value, span: Span) -> R<Value> {
        let prog = self.prog;
        match op {
            BinOp::Eq => return Ok(Value::Bool(self.values_equal(&l, &r, span)?)),
            BinOp::NotEq => return Ok(Value::Bool(!self.values_equal(&l, &r, span)?)),
            BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                // numbers compare directly (NaN makes every comparison false)
                if let (Some(a), Some(b)) = (num(&l), num(&r)) {
                    let both_int = matches!((&l, &r), (Value::Int(_), Value::Int(_)));
                    let ord = if both_int {
                        if let (Value::Int(x), Value::Int(y)) = (&l, &r) {
                            Some(x.cmp(y))
                        } else {
                            None
                        }
                    } else {
                        a.partial_cmp(&b)
                    };
                    return Ok(Value::Bool(ord_matches(op, ord)));
                }
                let ord = match (&l, &r) {
                    (Value::Str(a), Value::Str(b)) => Some(a.as_ref().cmp(b.as_ref())),
                    (Value::Uuid(a), Value::Uuid(b)) | (Value::Ulid(a), Value::Ulid(b)) => {
                        Some(a.cmp(b))
                    }
                    (Value::DateTime(a), Value::DateTime(b)) => Some(a.cmp(b)),
                    _ => match self.find_method(
                        &l,
                        Lang::Ord.method_name(),
                        Some(prog.defs.lang(Lang::Ord)),
                        Some(&r),
                    ) {
                        Some(fid) => {
                            match self.call_fid(fid, Some(l.clone()), vec![r.clone()], span)? {
                                Value::Int(i) => Some(i.cmp(&0)),
                                _ => return self.fail("R001", "`cmp` must return an int", span),
                            }
                        }
                        None => return self.fail("R004", "these values cannot be ordered", span),
                    },
                };
                return Ok(Value::Bool(ord_matches(op, ord)));
            }
            _ => {}
        }
        if let Some(v) = self.arith(op, &l, &r, span)? {
            return Ok(v);
        }
        let lang = match op {
            BinOp::Add => Lang::Add,
            BinOp::Sub => Lang::Sub,
            BinOp::Mul => Lang::Mul,
            BinOp::Div => Lang::Div,
            BinOp::IntDiv => Lang::IntDiv,
            BinOp::Mod => Lang::Mod,
            _ => Lang::Pow,
        };
        match self.find_method(&l, lang.method_name(), Some(prog.defs.lang(lang)), Some(&r)) {
            Some(fid) => self.call_fid(fid, Some(l), vec![r], span),
            None => self.fail(
                "R004",
                format!("cannot apply `{}` to these values", op.symbol()),
                span,
            ),
        }
    }

    fn arith(&mut self, op: BinOp, l: &Value, r: &Value, span: Span) -> R<Option<Value>> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                let (a, b) = (*a, *b);
                let overflow =
                    |this: &Self| this.fail::<Option<Value>>("R010", "integer overflow", span);
                let res = match op {
                    BinOp::Add => a.checked_add(b),
                    BinOp::Sub => a.checked_sub(b),
                    BinOp::Mul => a.checked_mul(b),
                    BinOp::Div => {
                        if b == 0 {
                            return self.fail("R011", "division by zero", span);
                        }
                        return Ok(Some(Value::Float(a as f64 / b as f64)));
                    }
                    BinOp::IntDiv => {
                        if b == 0 {
                            return self.fail("R011", "division by zero", span);
                        }
                        a.checked_div(b)
                    }
                    BinOp::Mod => {
                        if b == 0 {
                            return self.fail("R011", "division by zero", span);
                        }
                        a.checked_rem(b)
                    }
                    BinOp::Pow => {
                        if b < 0 {
                            return self.fail(
                                "R012",
                                "negative exponent: use decimals (`2.0 ** -1`) for fractions",
                                span,
                            );
                        }
                        match u32::try_from(b) {
                            Ok(e) => a.checked_pow(e),
                            Err(_) => None,
                        }
                    }
                    _ => return Ok(None),
                };
                match res {
                    Some(v) => Ok(Some(Value::Int(v))),
                    None => overflow(self),
                }
            }
            // the time between two instants, in milliseconds
            (Value::DateTime(a), Value::DateTime(b)) if op == BinOp::Sub => {
                Ok(Some(Value::Int(a - b)))
            }
            (Value::Str(a), Value::Str(b)) if op == BinOp::Add => {
                self.guard_alloc(a.len() + b.len(), span)?;
                self.charge_bytes(a.len() + b.len(), span)?;
                let mut s = String::with_capacity(a.len() + b.len());
                s.push_str(a);
                s.push_str(b);
                Ok(Some(Value::str(&s)))
            }
            _ => {
                let (Some(a), Some(b)) = (num(l), num(r)) else {
                    return Ok(None);
                };
                let v = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div | BinOp::Mod => {
                        if b == 0.0 {
                            return self.fail("R011", "division by zero", span);
                        }
                        if op == BinOp::Div { a / b } else { a % b }
                    }
                    BinOp::Pow => a.powf(b),
                    _ => return Ok(None),
                };
                Ok(Some(Value::Float(v)))
            }
        }
    }

    fn values_equal(&mut self, a: &Value, b: &Value, span: Span) -> R<bool> {
        let prog = self.prog;
        Ok(match (a, b) {
            (Value::None, Value::None) => true,
            (Value::None, _) | (_, Value::None) => false,
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Str(x), Value::Str(y)) => x == y,
            (Value::Uuid(x), Value::Uuid(y)) | (Value::Ulid(x), Value::Ulid(y)) => x == y,
            (Value::DateTime(x), Value::DateTime(y)) => x == y,
            (Value::Enum(e1, v1, d1), Value::Enum(e2, v2, d2)) => {
                if e1 != e2 || v1 != v2 {
                    return Ok(false);
                }
                match (d1, d2) {
                    (Some(xs), Some(ys)) if xs.len() == ys.len() => {
                        let (xs, ys) = (xs.clone(), ys.clone());
                        for (x, y) in xs.iter().zip(ys.iter()) {
                            if !self.values_equal(x, y, span)? {
                                return Ok(false);
                            }
                        }
                        true
                    }
                    (None, None) => true,
                    _ => false,
                }
            }
            (Value::Map(x), Value::Map(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                let xs: Vec<(Value, Value)> = x.borrow().entries.clone();
                if xs.len() != y.borrow().len() {
                    return Ok(false);
                }
                for (k, v) in &xs {
                    let other = y.borrow().get(k).cloned();
                    match other {
                        Some(o) => {
                            if !self.values_equal(v, &o, span)? {
                                return Ok(false);
                            }
                        }
                        None => return Ok(false),
                    }
                }
                true
            }
            (Value::Array(x), Value::Array(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                let n = x.borrow().len();
                self.charge(n, span)?;
                let (xs, ys): (Vec<Value>, Vec<Value>) = (x.borrow().clone(), y.borrow().clone());
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                for (p, q) in xs.iter().zip(ys.iter()) {
                    if !self.values_equal(p, q, span)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Struct(x), Value::Struct(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
                match self.find_method(
                    a,
                    Lang::Eq.method_name(),
                    Some(prog.defs.lang(Lang::Eq)),
                    Some(b),
                ) {
                    Some(fid) => {
                        match self.call_fid(fid, Some(a.clone()), vec![b.clone()], span)? {
                            Value::Bool(r) => r,
                            _ => return self.fail("R001", "`eq` must return a bool", span),
                        }
                    }
                    None => false,
                }
            }
            _ => match (num(a), num(b)) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            },
        })
    }

    // ----------------------------------------------------------------- places

    fn index_get(&mut self, base: &Value, idx: &Value, span: Span) -> R<Value> {
        match (base, idx) {
            (Value::Array(a), Value::Int(i)) => {
                let a = a.borrow();
                match usize::try_from(*i).ok().and_then(|u| a.get(u)) {
                    Some(v) => Ok(v.clone()),
                    None => self.fail(
                        "R020",
                        format!("index {i} is out of bounds (length {})", a.len()),
                        span,
                    ),
                }
            }
            (Value::Map(m), k) => Ok(m.borrow().get(k).cloned().unwrap_or(Value::None)),
            (Value::Str(s), Value::Int(i)) => {
                // finding the i-th character walks the string
                self.charge_bytes((*i).max(0) as usize, span)?;
                match usize::try_from(*i).ok().and_then(|u| s.chars().nth(u)) {
                    Some(c) => Ok(Value::str(&c.to_string())),
                    None => self.fail(
                        "R020",
                        format!("index {i} is out of bounds (length {})", s.chars().count()),
                        span,
                    ),
                }
            }
            _ => match self.find_method(
                base,
                Lang::Index.method_name(),
                Some(self.prog.defs.lang(Lang::Index)),
                Some(idx),
            ) {
                Some(fid) => self.call_fid(fid, Some(base.clone()), vec![idx.clone()], span),
                None => self.fail("R004", "this value cannot be indexed", span),
            },
        }
    }

    fn eval_place(&mut self, target: &Expr, env: &Env) -> R<Place> {
        let prog = self.prog;
        match &target.kind {
            ExprKind::Ident(name, _) => Ok(match prog.res.paths.get(&target.id) {
                Some(PathRes::Global(m, n)) => Place::Global(*m, n.clone()),
                _ => Place::Local(name.clone()),
            }),
            ExprKind::Field(obj, name, _) => {
                if let Some(PathRes::StaticField(sid, n)) = prog.res.paths.get(&target.id) {
                    return Ok(Place::Static(*sid, n.clone()));
                }
                match self.eval(obj, env)? {
                    Value::Struct(o) => Ok(Place::Field(o, name.name.clone())),
                    Value::None => self.fail(
                        "R006",
                        format!("cannot set `.{}`: the value is `none`", name.name),
                        target.span,
                    ),
                    _ => self.fail(
                        "R005",
                        format!("this value has no field `{}`", name.name),
                        name.span,
                    ),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.eval(base, env)?;
                let i = self.eval(idx, env)?;
                match (&b, &i) {
                    (Value::Map(m), _) => Ok(Place::MapEntry(m.clone(), i)),
                    (Value::Array(a), Value::Int(n)) => {
                        let len = a.borrow().len();
                        match usize::try_from(*n).ok().filter(|u| *u < len) {
                            Some(u) => Ok(Place::Elem(a.clone(), u)),
                            None => self.fail(
                                "R020",
                                format!("index {n} is out of bounds (length {len})"),
                                target.span,
                            ),
                        }
                    }
                    _ => Ok(Place::UserIndex(b, i)),
                }
            }
            ExprKind::Append(base) => Ok(Place::Append(self.eval(base, env)?)),
            _ => self.fail("R003", "invalid assignment target", target.span),
        }
    }

    fn place_get(&mut self, p: &Place, env: &Env, span: Span) -> R<Value> {
        match p {
            Place::Local(n) => match env.get(n) {
                Some(v) => Ok(v),
                None => self.fail("R002", format!("`{n}` has no value yet"), span),
            },
            Place::Global(m, n) => match self.envs[*m].get(n) {
                Some(v) => Ok(v),
                None => self.fail("R002", format!("`{n}` has no value yet"), span),
            },
            Place::Field(o, n) => match o.fields.borrow().get(n) {
                Some(v) => Ok(v.clone()),
                None => self.fail("R005", format!("this value has no field `{n}`"), span),
            },
            Place::Static(sid, n) => match self.statics.get(&(*sid, n.clone())) {
                Some(v) => Ok(v.clone()),
                None => self.fail("R002", format!("static field `{n}` has no value yet"), span),
            },
            Place::Elem(a, i) => Ok(a.borrow()[*i].clone()),
            Place::MapEntry(m, k) => match m.borrow().get(k) {
                Some(v) => Ok(v.clone()),
                None => self.fail("R022", "this key is not in the map (use `get(key, default)` or check `has(key)` first)", span),
            },
            Place::UserIndex(b, i) => self.index_get(b, i, span),
            Place::Append(_) => self.fail("R003", "`x[]` cannot be read", span),
        }
    }

    fn place_set(&mut self, p: &Place, v: Value, env: &Env, span: Span) -> R<()> {
        let prog = self.prog;
        match p {
            Place::Local(n) => {
                if !env.assign(n, v) {
                    return self.fail("R002", format!("`{n}` has no value yet"), span);
                }
            }
            Place::Global(m, n) => {
                if !self.envs[*m].assign(n, v) {
                    return self.fail("R002", format!("`{n}` has no value yet"), span);
                }
            }
            Place::Field(o, n) => {
                o.fields.borrow_mut().insert(n, v);
            }
            Place::Static(sid, n) => {
                self.statics.insert((*sid, n.clone()), v);
            }
            Place::Elem(a, i) => {
                a.borrow_mut()[*i] = v;
            }
            Place::MapEntry(m, k) => {
                let len = m.borrow().len();
                self.guard_alloc(len + 1, span)?;
                if !m.borrow_mut().insert(k.clone(), v) {
                    return self.fail("R023", "this value cannot be used as a map key", span);
                }
            }
            Place::UserIndex(b, i) => match b {
                Value::Array(a) => {
                    let Value::Int(n) = i else {
                        return self.fail("R020", "an array index must be an int", span);
                    };
                    let len = a.borrow().len();
                    match usize::try_from(*n).ok().filter(|u| *u < len) {
                        Some(u) => a.borrow_mut()[u] = v,
                        None => {
                            return self.fail(
                                "R020",
                                format!("index {n} is out of bounds (length {len})"),
                                span,
                            );
                        }
                    }
                }
                _ => match self.find_method(
                    b,
                    Lang::IndexSet.method_name(),
                    Some(prog.defs.lang(Lang::IndexSet)),
                    Some(i),
                ) {
                    Some(fid) => {
                        self.call_fid(fid, Some(b.clone()), vec![i.clone(), v], span)?;
                    }
                    None => {
                        return self.fail("R004", "this value cannot be assigned by index", span);
                    }
                },
            },
            Place::Append(b) => match b {
                Value::Array(a) => {
                    self.guard_alloc(a.borrow().len() + 1, span)?;
                    a.borrow_mut().push(v)
                }
                _ => match self.find_method(
                    b,
                    Lang::Push.method_name(),
                    Some(prog.defs.lang(Lang::Push)),
                    Some(&v),
                ) {
                    Some(fid) => {
                        self.call_fid(fid, Some(b.clone()), vec![v], span)?;
                    }
                    None => return self.fail("R004", "this value cannot be appended to", span),
                },
            },
        }
        Ok(())
    }

    fn eval_assign(
        &mut self,
        e: &Expr,
        op: Option<BinOp>,
        target: &Expr,
        value: &Expr,
        env: &Env,
    ) -> R<Value> {
        // the common case, a variable, without building a `Place` (which owns its name)
        if let ExprKind::Ident(name, _) = &target.kind {
            let prog = self.prog;
            let (scope, key): (Env, &str) = match prog.res.paths.get(&target.id) {
                Some(PathRes::Global(m, n)) => (self.envs[*m].clone(), n.as_str()),
                _ => (env.clone(), name.as_str()),
            };
            let v = match op {
                None => self.eval(value, env)?,
                Some(bop) => {
                    let Some(cur) = scope.get(key) else {
                        return self.fail("R002", format!("`{key}` has no value yet"), target.span);
                    };
                    let rhs = self.eval(value, env)?;
                    self.binary(bop, cur, rhs, e.span)?
                }
            };
            if !scope.assign(key, v.clone()) {
                return self.fail("R002", format!("`{key}` has no value yet"), target.span);
            }
            return Ok(v);
        }
        let place = self.eval_place(target, env)?;
        let v = match op {
            None => self.eval(value, env)?,
            Some(bop) => {
                let cur = self.place_get(&place, env, target.span)?;
                let rhs = self.eval(value, env)?;
                self.binary(bop, cur, rhs, e.span)?
            }
        };
        self.place_set(&place, v.clone(), env, e.span)?;
        Ok(v)
    }

    // --------------------------------------------------------------- patterns

    fn match_pattern(
        &mut self,
        p: &Pattern,
        v: &Value,
        mid: NodeId,
        idx: usize,
        scope: &Env,
    ) -> R<bool> {
        Ok(match p {
            Pattern::Int(n) => {
                matches!(v, Value::Int(x) if x == n)
                    || matches!(v, Value::Float(f) if *f == *n as f64)
            }
            Pattern::Float(f) => num(v).is_some_and(|x| x == *f),
            Pattern::Str(s) => matches!(v, Value::Str(x) if x.as_ref() == s),
            Pattern::Bool(b) => matches!(v, Value::Bool(x) if x == b),
            Pattern::None => matches!(v, Value::None),
            Pattern::Range { lo, hi, inclusive } => {
                let bound = |p: &Pattern| match p {
                    Pattern::Int(n) => *n as f64,
                    Pattern::Float(f) => *f,
                    _ => f64::NAN,
                };
                let (Some(x), l, h) = (num(v), bound(lo), bound(hi)) else {
                    return Ok(false);
                };
                x >= l && if *inclusive { x <= h } else { x < h }
            }
            Pattern::Path(_) => match (self.prog.res.patterns.get(&(mid, idx)), v) {
                (Some((eid, vi)), Value::Enum(e, x, _)) => eid == e && vi == x,
                _ => false,
            },
            Pattern::Variant { fields, .. } => match (self.prog.res.patterns.get(&(mid, idx)), v) {
                (Some((eid, vi)), Value::Enum(e, x, Some(data))) if eid == e && vi == x => {
                    for (bind, value) in fields.iter().zip(data.iter()) {
                        if let Some(name) = bind {
                            scope.define(&name.name, value.clone());
                        }
                    }
                    true
                }
                _ => false,
            },
            Pattern::Bind(name) => {
                // a name that is a variant of the type matched (decided by the checker)
                if let Some((eid, vi)) = self.prog.res.patterns.get(&(mid, idx)) {
                    return Ok(matches!(v, Value::Enum(e, x, _) if e == eid && x == vi));
                }
                if name.name != "_" {
                    scope.define(&name.name, v.clone());
                }
                true
            }
        })
    }

    // -------------------------------------------------------------- built-ins

    /// Total order used by `sort`, `min` and `max`.
    fn order(&mut self, a: &Value, b: &Value, span: Span) -> R<Ordering> {
        self.tick(span)?;
        match (a, b) {
            (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
            (Value::Str(x), Value::Str(y)) => Ok(x.as_ref().cmp(y.as_ref())),
            (Value::Uuid(x), Value::Uuid(y)) | (Value::Ulid(x), Value::Ulid(y)) => Ok(x.cmp(y)),
            (Value::DateTime(x), Value::DateTime(y)) => Ok(x.cmp(y)),
            _ => {
                if let (Some(x), Some(y)) = (num(a), num(b)) {
                    return Ok(x.total_cmp(&y));
                }
                let ord = self.prog.defs.lang(Lang::Ord);
                match self.find_method(a, Lang::Ord.method_name(), Some(ord), Some(b)) {
                    Some(fid) => {
                        match self.call_fid(fid, Some(a.clone()), vec![b.clone()], span)? {
                            Value::Int(i) => Ok(i.cmp(&0)),
                            _ => self.fail("R001", "`cmp` must return an int", span),
                        }
                    }
                    None => self.fail("R004", "these values cannot be ordered", span),
                }
            }
        }
    }

    /// Stable merge sort, because comparing can call user code and fail.
    fn merge_sort(&mut self, v: Vec<Value>, span: Span) -> R<Vec<Value>> {
        if v.len() <= 1 {
            return Ok(v);
        }
        let mid = v.len() / 2;
        let left = self.merge_sort(v[..mid].to_vec(), span)?;
        let right = self.merge_sort(v[mid..].to_vec(), span)?;
        let mut out = Vec::with_capacity(left.len() + right.len());
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            if self.order(&left[i], &right[j], span)? != Ordering::Greater {
                out.push(left[i].clone());
                i += 1;
            } else {
                out.push(right[j].clone());
                j += 1;
            }
        }
        out.extend_from_slice(&left[i..]);
        out.extend_from_slice(&right[j..]);
        Ok(out)
    }

    /// Same as `merge_sort`, but ordering through a qlang comparator (`sort_by`).
    fn merge_sort_by(&mut self, v: Vec<Value>, cmp: &Value, span: Span) -> R<Vec<Value>> {
        if v.len() <= 1 {
            return Ok(v);
        }
        let mid = v.len() / 2;
        let left = self.merge_sort_by(v[..mid].to_vec(), cmp, span)?;
        let right = self.merge_sort_by(v[mid..].to_vec(), cmp, span)?;
        let mut out = Vec::with_capacity(left.len() + right.len());
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            let ord = match self.call_value(cmp, vec![left[i].clone(), right[j].clone()], span)? {
                Value::Int(n) => n,
                _ => {
                    return self.fail(
                        "R001",
                        "the comparator passed to `sort_by` must return an int",
                        span,
                    );
                }
            };
            if ord <= 0 {
                out.push(left[i].clone());
                i += 1;
            } else {
                out.push(right[j].clone());
                j += 1;
            }
        }
        out.extend_from_slice(&left[i..]);
        out.extend_from_slice(&right[j..]);
        Ok(out)
    }

    #[inline(never)]
    fn builtin_fn(&mut self, name: &str, args: Vec<Value>, span: Span) -> R<Value> {
        match name {
            "print" | "write" => {
                let mut text = match args.first() {
                    Some(v) => self.stringify(v, span)?,
                    None => String::new(),
                };
                if name == "print" {
                    text.push('\n');
                }
                self.emit(&text, span)?;
                Ok(Value::Unit)
            }
            "read" => match self.host.read_line() {
                Ok(Some(line)) => Ok(Value::str(&line)),
                Ok(None) => self.fail("R040", "no more input to read", span),
                Err(HostError::Unsupported(_)) => {
                    self.fail("R041", "`read` is not available in this environment", span)
                }
                Err(HostError::Other(m)) => {
                    self.fail("R041", format!("cannot read input: {m}"), span)
                }
            },
            "read_all" => {
                // every remaining line, joined by new lines (no new line after the last)
                let mut lines: Vec<String> = Vec::new();
                loop {
                    match self.host.read_line() {
                        Ok(Some(line)) => {
                            self.charge_bytes(line.len(), span)?;
                            lines.push(line);
                            self.guard_alloc(lines.len(), span)?;
                        }
                        Ok(None) => break,
                        Err(HostError::Unsupported(_)) => {
                            return self.fail(
                                "R041",
                                "`read_all` is not available in this environment",
                                span,
                            );
                        }
                        Err(HostError::Other(m)) => {
                            return self.fail("R041", format!("cannot read input: {m}"), span);
                        }
                    }
                }
                let text = lines.join("\n");
                self.guard_alloc(text.len(), span)?;
                Ok(Value::str(&text))
            }
            "exit" => {
                let code = match args.first() {
                    Some(Value::Int(n)) => (*n).clamp(0, 255) as i32,
                    _ => 0,
                };
                Err(Ctrl::Exit(code))
            }
            "panic" => {
                let msg = self.stringify(&args[0], span)?;
                self.fail("R050", format!("panic: {msg}"), span)
            }
            _ => match args.first() {
                Some(Value::Bool(true)) => Ok(Value::Unit),
                _ => {
                    let msg = match args.get(1) {
                        Some(m) => format!("assertion failed: {}", self.stringify(m, span)?),
                        None => "assertion failed".to_string(),
                    };
                    self.fail("R051", msg, span)
                }
            },
        }
    }

    /// `Base64.*`, `Hex.*`, `Url.*` and `Hash.*`: encodings and hashes of the UTF-8 bytes of
    /// a string. Decoding gives `none` unless the bytes are valid text.
    #[inline(never)]
    fn codec_call(&mut self, ty: &str, name: &str, args: &[Value], span: Span) -> R<Value> {
        use crate::codec;
        let text = |v: &Value| match v {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        };
        let input = text(&args[0]);
        // hashing and encoding read every byte: charged like other built-in work
        self.charge(input.len(), span)?;
        let out = match (ty, name) {
            ("Base64", "encode") => codec::base64_encode(input.as_bytes(), false),
            ("Base64", "encode_url") => codec::base64_encode(input.as_bytes(), true),
            ("Hex", "encode") => codec::hex_encode(input.as_bytes()),
            ("Url", "encode") => codec::url_encode(input.as_bytes()),
            ("Hash", "sha256") => codec::hex_encode(&codec::sha256(input.as_bytes())),
            ("Hash", "sha1") => codec::hex_encode(&codec::sha1(input.as_bytes())),
            ("Hash", "md5") => codec::hex_encode(&codec::md5(input.as_bytes())),
            ("Hash", "hmac_sha256") => {
                let message = text(&args[1]);
                self.charge(message.len(), span)?;
                codec::hex_encode(&codec::hmac_sha256(input.as_bytes(), message.as_bytes()))
            }
            (_, "decode") => {
                let bytes = match ty {
                    "Base64" => codec::base64_decode(&input),
                    "Hex" => codec::hex_decode(&input),
                    _ => codec::url_decode(&input),
                };
                return Ok(bytes
                    .and_then(|b| String::from_utf8(b).ok())
                    .map_or(Value::None, |s| Value::str(&s)));
            }
            _ => return self.fail("R001", format!("unknown function `{ty}.{name}`"), span),
        };
        self.guard_alloc(out.len(), span)?;
        Ok(Value::str(&out))
    }

    /// A function of a built-in type: `int.parse`, `uuid.v4`, `time.now`, `random.int`...
    /// Not inlined: it would make the frame of `eval_call` (used by every recursion) bigger.
    #[inline(never)]
    fn static_call(&mut self, ty: &str, name: &str, args: Vec<Value>, span: Span) -> R<Value> {
        let text = |v: &Value| match v {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        };
        Ok(match (ty, name) {
            ("int" | "float", "parse") => return self.parse_number(ty, &args[0]),
            ("uuid", "parse") => ids::parse_uuid(&text(&args[0])).map_or(Value::None, Value::Uuid),
            ("ulid", "parse") => ids::parse_ulid(&text(&args[0])).map_or(Value::None, Value::Ulid),
            ("time", "now") => Value::Int(self.now(span)?),
            ("time", "sleep") => {
                let ms = match args.first() {
                    Some(Value::Int(n)) => *n,
                    _ => 0,
                };
                if !(0..=3_600_000).contains(&ms) {
                    return self.fail(
                        "R017",
                        format!("`time.sleep` waits from 0 to 3600000 milliseconds (found {ms})"),
                        span,
                    );
                }
                match self.host.sleep_ms(ms as u64) {
                    Ok(()) => Value::Unit,
                    Err(HostError::Unsupported(_)) => {
                        return self.fail(
                            "R060",
                            "waiting is not available in this environment",
                            span,
                        );
                    }
                    Err(HostError::Other(m)) => {
                        return self.fail("R060", format!("cannot wait: {m}"), span);
                    }
                }
            }
            ("datetime", "now") => {
                Value::DateTime(self.now(span)?.clamp(datetime::MIN_MS, datetime::MAX_MS))
            }
            ("datetime", "parse") => {
                datetime::parse(&text(&args[0])).map_or(Value::None, Value::DateTime)
            }
            ("datetime", "at") => {
                let Value::Int(ms) = args[0] else {
                    return Ok(Value::None);
                };
                if !datetime::in_range(ms) {
                    return self.fail(
                        "R064",
                        "a datetime must be between the years 1 and 9999",
                        span,
                    );
                }
                Value::DateTime(ms)
            }
            ("datetime", "of") => {
                let n: Vec<i64> = args
                    .iter()
                    .map(|a| if let Value::Int(i) = a { *i } else { 0 })
                    .collect();
                let at = |i: usize| n.get(i).copied().unwrap_or(0);
                match datetime::from_parts(at(0), at(1), at(2), at(3), at(4), at(5), 0) {
                    Some(t) => Value::DateTime(t),
                    None => {
                        return self.fail(
                            "R064",
                            format!(
                                "this date does not exist: {}",
                                n.iter()
                                    .map(|x| x.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            span,
                        );
                    }
                }
            }
            ("uuid", "v4") => Value::Uuid(ids::uuid_v4(self.random_bits(span)?)),
            ("uuid", "v7") => {
                let now = self.now(span)?;
                Value::Uuid(ids::uuid_v7(now as u128, self.random_bits(span)?))
            }
            ("ulid", "at") => {
                let Value::Int(t) = args[0] else {
                    return Ok(Value::None);
                };
                if !(0..=ids::TIME_MASK as i64).contains(&t) {
                    return self.fail("R061", "a ulid can only hold a time between 0 and 281 474 976 710 655 milliseconds", span);
                }
                Value::Ulid(ids::make_ulid(t as u128, self.random_bits(span)?))
            }
            ("ulid", "new") => {
                let now = self.now(span)?.max(0) as u128;
                let mut id = ids::make_ulid(now, self.random_bits(span)?);
                // the same millisecond (or a clock that went back): count up from the last one
                if id <= self.last_ulid && self.last_ulid < u128::MAX {
                    id = self.last_ulid + 1;
                }
                self.last_ulid = id;
                Value::Ulid(id)
            }
            ("random", "float") => {
                Value::Float((self.random(span)? >> 11) as f64 / (1u64 << 53) as f64)
            }
            ("random", "int") => {
                let (Value::Int(lo), Value::Int(hi)) = (&args[0], &args[1]) else {
                    return Ok(Value::None);
                };
                if lo > hi {
                    return self.fail(
                        "R062",
                        format!("random.int needs min <= max, found {lo} and {hi}"),
                        span,
                    );
                }
                let width = (*hi as i128 - *lo as i128 + 1) as u128;
                let draw = self.random_bits(span)? % width;
                Value::Int((*lo as i128 + draw as i128) as i64)
            }
            ("Base64" | "Hex" | "Url" | "Hash", _) => {
                return self.codec_call(ty, name, &args, span);
            }
            ("regex", "matches") => Value::Bool(
                self.compile_regex(&text(&args[1]), span)?
                    .is_match(&text(&args[0])),
            ),
            ("regex", "find") => match self
                .compile_regex(&text(&args[1]), span)?
                .find(&text(&args[0]))
            {
                Some(m) => Value::str(m.as_str()),
                None => Value::None,
            },
            ("regex", "find_all") => {
                let re = self.compile_regex(&text(&args[1]), span)?;
                let matches: Vec<Value> = re
                    .find_iter(&text(&args[0]))
                    .map(|m| Value::str(m.as_str()))
                    .collect();
                Value::array(matches)
            }
            ("regex", "captures") => {
                let re = self.compile_regex(&text(&args[1]), span)?;
                match re.captures(&text(&args[0])) {
                    Some(caps) => {
                        let groups: Vec<Value> = caps
                            .iter()
                            .map(|g| g.map_or(Value::None, |m| Value::str(m.as_str())))
                            .collect();
                        Value::array(groups)
                    }
                    None => Value::None,
                }
            }
            ("regex", "replace") => Value::str(
                &self
                    .compile_regex(&text(&args[1]), span)?
                    .replace(&text(&args[0]), text(&args[2]).as_str()),
            ),
            ("regex", "replace_all") => Value::str(
                &self
                    .compile_regex(&text(&args[1]), span)?
                    .replace_all(&text(&args[0]), text(&args[2]).as_str()),
            ),
            ("regex", "split") => {
                let re = self.compile_regex(&text(&args[1]), span)?;
                let parts: Vec<Value> = re.split(&text(&args[0])).map(Value::str).collect();
                Value::array(parts)
            }
            _ => Value::Unit,
        })
    }

    /// Compiles a regular expression; a malformed pattern stops the program (like a
    /// malformed date in `datetime.of`), it is not something to recover from at run time.
    fn compile_regex(&self, pattern: &str, span: Span) -> R<regex::Regex> {
        regex::Regex::new(pattern)
            .map_err(|e| self.error("R070", format!("invalid regular expression: {e}"), span))
    }

    /// The id of a struct declared in the prelude, by name.
    fn prelude_struct(&self, name: &str) -> StructId {
        self.prog
            .defs
            .structs
            .iter()
            .position(|s| s.module == crate::check::PRELUDE_MOD && s.name == name)
            .unwrap_or_else(|| panic!("prelude struct `{name}` not found"))
    }

    fn new_struct(&self, sid: StructId, fields: &[(&str, Value)]) -> Value {
        let mut map = SmallMap::default();
        for (k, v) in fields {
            map.insert(k, v.clone());
        }
        Value::Struct(Rc::new(StructObj {
            def: sid,
            fields: RefCell::new(map),
        }))
    }

    /// Whether `v`, a `Result`, is the `Err` variant (`vi` is its variant index).
    fn is_err_variant(&self, v: &Value, vi: usize) -> bool {
        let Value::Enum(eid, ..) = v else {
            return false;
        };
        self.prog.defs.enums[*eid].variants[vi].name == "Err"
    }

    fn prelude_enum(&self, name: &str) -> EnumId {
        self.prog
            .defs
            .enums
            .iter()
            .position(|e| e.module == crate::check::PRELUDE_MOD && e.name == name)
            .unwrap_or_else(|| panic!("prelude enum `{name}` not found"))
    }

    /// Builds `Ok(data)`, a value of the prelude's `Result`.
    fn ok_result(&self, data: Value) -> Value {
        let eid = self.prelude_enum("Result");
        let vi = self.prog.defs.enums[eid]
            .variant_index("Ok")
            .expect("Result has Ok");
        Value::Enum(eid, vi, Some(Rc::from(vec![data])))
    }

    /// Builds `Err(Error { message })`.
    fn err_result(&self, message: &str) -> Value {
        let error = self.new_struct(
            self.prelude_struct("Error"),
            &[("message", Value::str(message))],
        );
        let eid = self.prelude_enum("Result");
        let vi = self.prog.defs.enums[eid]
            .variant_index("Err")
            .expect("Result has Err");
        Value::Enum(eid, vi, Some(Rc::from(vec![error])))
    }

    /// Turns a [`HostError`] into a `Result::err`: a host without the capability, or one
    /// that failed for its own reasons, are both reported the same way to qlang code.
    fn host_err_result(&self, err: HostError) -> Value {
        match err {
            HostError::Unsupported(what) => {
                self.err_result(&format!("{what} is not available in this environment"))
            }
            HostError::Other(m) => self.err_result(&m),
        }
    }

    /// A `map<string, string>` value from `(name, value)` pairs.
    fn string_map(pairs: &[(String, String)]) -> Value {
        let mut m = MapObj::default();
        for (k, v) in pairs {
            m.insert(Value::str(k), Value::str(v));
        }
        Value::Map(Rc::new(RefCell::new(m)))
    }

    #[inline(never)]
    fn datetime_method(
        &mut self,
        t: i64,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> R<Option<Value>> {
        let p = datetime::parts(t);
        let n = match args.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        let moved = |this: &mut Self, r: Option<i64>| -> R<Option<Value>> {
            match r.filter(|ms| datetime::in_range(*ms)) {
                Some(ms) => Ok(Some(Value::DateTime(ms))),
                None => this.fail("R065", "the result is out of the years 1 to 9999", span),
            }
        };
        Ok(Some(match name {
            "year" => Value::Int(p.year),
            "month" => Value::Int(p.month),
            "day" => Value::Int(p.day),
            "hour" => Value::Int(p.hour),
            "minute" => Value::Int(p.minute),
            "second" => Value::Int(p.second),
            "weekday" => Value::Int(datetime::weekday(t)),
            "ms" => Value::Int(t),
            "start_of_day" => Value::DateTime(datetime::start_of_day(t)),
            "add_years" => {
                return moved(
                    self,
                    n.checked_mul(12).and_then(|m| datetime::add_months(t, m)),
                );
            }
            "add_months" => return moved(self, datetime::add_months(t, n)),
            "add_days" => {
                return moved(
                    self,
                    n.checked_mul(datetime::DAY_MS)
                        .and_then(|d| t.checked_add(d)),
                );
            }
            "add_hours" => {
                return moved(
                    self,
                    n.checked_mul(3_600_000).and_then(|d| t.checked_add(d)),
                );
            }
            "add_minutes" => {
                return moved(self, n.checked_mul(60_000).and_then(|d| t.checked_add(d)));
            }
            "add_seconds" => {
                return moved(self, n.checked_mul(1000).and_then(|d| t.checked_add(d)));
            }
            "days_until" => match args.first() {
                Some(Value::DateTime(o)) => Value::Int(
                    (datetime::start_of_day(*o) - datetime::start_of_day(t)) / datetime::DAY_MS,
                ),
                _ => return Ok(None),
            },
            "format" => match args.first() {
                Some(Value::Str(pattern)) => {
                    self.guard_alloc(pattern.len() * 4, span)?;
                    Value::str(&datetime::format(t, pattern))
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        }))
    }

    fn now(&mut self, span: Span) -> R<i64> {
        match self.host.now_ms() {
            Ok(t) => Ok(t),
            Err(HostError::Unsupported(_)) => self.fail(
                "R060",
                "the time is not available in this environment",
                span,
            ),
            Err(HostError::Other(m)) => {
                self.fail("R060", format!("cannot read the time: {m}"), span)
            }
        }
    }

    fn random(&mut self, span: Span) -> R<u64> {
        match self.host.random_u64() {
            Ok(r) => Ok(r),
            Err(HostError::Unsupported(_)) => self.fail(
                "R063",
                "randomness is not available in this environment",
                span,
            ),
            Err(HostError::Other(m)) => {
                self.fail("R063", format!("cannot get random numbers: {m}"), span)
            }
        }
    }

    /// 128 random bits.
    fn random_bits(&mut self, span: Span) -> R<u128> {
        Ok(((self.random(span)? as u128) << 64) | self.random(span)? as u128)
    }

    fn parse_number(&mut self, ty: &str, v: &Value) -> R<Value> {
        let Value::Str(s) = v else {
            return Ok(Value::None);
        };
        let t = s.trim();
        Ok(if ty == "int" {
            match t.replace('_', "").parse::<i64>() {
                Ok(i) if !t.is_empty() && !t.starts_with('_') && !t.ends_with('_') => Value::Int(i),
                _ => Value::None,
            }
        } else {
            match t.parse::<f64>() {
                Ok(f) if f.is_finite() && !t.is_empty() => Value::Float(f),
                _ => Value::None,
            }
        })
    }

    fn builtin_method(
        &mut self,
        recv: &Value,
        name: &str,
        args: &[Value],
        span: Span,
    ) -> R<Option<Value>> {
        // linear built-ins pay for the elements they visit
        match recv {
            Value::Str(s) => self.charge_bytes(s.len(), span)?,
            Value::Array(a)
                if matches!(
                    name,
                    "contains"
                        | "index_of"
                        | "join"
                        | "reverse"
                        | "insert"
                        | "remove"
                        | "sum"
                        | "min"
                        | "max"
                        | "slice"
                        | "sort"
                        | "unique"
                        | "flatten"
                        | "take"
                        | "drop"
                        | "chunk"
                ) =>
            {
                let n = a.borrow().len();
                self.charge(n, span)?;
            }
            Value::Map(m) if matches!(name, "keys" | "values" | "remove") => {
                let n = m.borrow().len();
                self.charge(n, span)?;
            }
            _ => {}
        }
        let int_arg = |i: usize| -> Option<i64> {
            match args.get(i) {
                Some(Value::Int(n)) => Some(*n),
                _ => None,
            }
        };
        let str_arg = |i: usize| -> Option<Rc<str>> {
            match args.get(i) {
                Some(Value::Str(s)) => Some(s.clone()),
                _ => None,
            }
        };
        Ok(Some(match (recv, name) {
            (Value::Array(a), "len") => Value::Int(a.borrow().len() as i64),
            (Value::Array(a), "is_empty") => Value::Bool(a.borrow().is_empty()),
            (Value::Array(a), "push") => {
                self.guard_alloc(a.borrow().len() + 1, span)?;
                a.borrow_mut().push(args[0].clone());
                Value::Unit
            }
            (Value::Array(a), "pop") => a.borrow_mut().pop().unwrap_or(Value::None),
            (Value::Array(a), "insert") => {
                let len = a.borrow().len();
                match int_arg(0)
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|i| *i <= len)
                {
                    Some(i) => {
                        self.guard_alloc(len + 1, span)?;
                        a.borrow_mut().insert(i, args[1].clone());
                        Value::Unit
                    }
                    None => {
                        return self.fail(
                            "R020",
                            format!("cannot insert at this index (length {len})"),
                            span,
                        );
                    }
                }
            }
            (Value::Array(a), "remove") => {
                let len = a.borrow().len();
                match int_arg(0)
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|i| *i < len)
                {
                    Some(i) => a.borrow_mut().remove(i),
                    None => {
                        return self.fail(
                            "R020",
                            format!("cannot remove at this index (length {len})"),
                            span,
                        );
                    }
                }
            }
            (Value::Array(a), "clear") => {
                a.borrow_mut().clear();
                Value::Unit
            }
            (Value::Array(a), "contains") | (Value::Array(a), "index_of") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut found = None;
                for (i, it) in items.iter().enumerate() {
                    if self.values_equal(it, &args[0], span)? {
                        found = Some(i);
                        break;
                    }
                }
                if name == "contains" {
                    Value::Bool(found.is_some())
                } else {
                    found.map_or(Value::None, |i| Value::Int(i as i64))
                }
            }
            (Value::Array(a), "reverse") => {
                a.borrow_mut().reverse();
                Value::Unit
            }
            (Value::Array(a), "join") => {
                let items: Vec<Value> = a.borrow().clone();
                let sep = str_arg(0).unwrap_or_else(|| Rc::from(""));
                let mut parts = Vec::new();
                for it in &items {
                    parts.push(self.stringify(it, span)?);
                }
                let joined = parts.join(&sep);
                self.guard_alloc(joined.len(), span)?;
                Value::str(&joined)
            }
            (Value::DateTime(t), _) => match self.datetime_method(*t, name, args, span)? {
                Some(v) => v,
                None => return Ok(None),
            },
            (Value::Uuid(u), "version") => Value::Int(ids::uuid_version(*u)),
            (Value::Ulid(u), "time") => Value::Int(ids::ulid_time(*u)),
            (Value::Str(s), "len") => Value::Int(s.chars().count() as i64),
            (Value::Str(s), "is_empty") => Value::Bool(s.is_empty()),
            (Value::Str(s), "trim_start") => Value::str(s.trim_start()),
            (Value::Str(s), "trim_end") => Value::str(s.trim_end()),
            (Value::Str(s), "reverse") => Value::str(&s.chars().rev().collect::<String>()),
            (Value::Str(s), "capitalize") => {
                let mut cs = s.chars();
                match cs.next() {
                    Some(first) => {
                        Value::str(&(first.to_uppercase().collect::<String>() + cs.as_str()))
                    }
                    None => Value::Str(s.clone()),
                }
            }
            (Value::Str(s), "last_index_of") => match s.rfind(str_arg(0).unwrap().as_ref()) {
                Some(b) => Value::Int(s[..b].chars().count() as i64),
                None => Value::None,
            },
            (Value::Str(s), "count") => {
                let needle = str_arg(0).unwrap();
                if needle.is_empty() {
                    return self.fail("R021", "cannot count occurrences of an empty string", span);
                }
                Value::Int(s.matches(needle.as_ref()).count() as i64)
            }
            (Value::Str(s), "lines") => Value::array(s.lines().map(Value::str).collect()),
            (Value::Str(s), "words") => {
                Value::array(s.split_whitespace().map(Value::str).collect())
            }
            (Value::Str(s), "pad_left" | "pad_right") => {
                let fill = str_arg(1).unwrap();
                let mut fc = fill.chars();
                let (Some(f), None) = (fc.next(), fc.next()) else {
                    return self.fail("R021", "the padding must be exactly one character", span);
                };
                let width = int_arg(0).unwrap_or(0).max(0) as usize;
                self.guard_alloc(width, span)?;
                let n = s.chars().count();
                if n >= width {
                    Value::Str(s.clone())
                } else {
                    let pad: String = std::iter::repeat_n(f, width - n).collect();
                    Value::str(&if name == "pad_left" {
                        format!("{pad}{s}")
                    } else {
                        format!("{s}{pad}")
                    })
                }
            }
            (Value::Str(s), "is_digit") => {
                Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            }
            (Value::Str(s), "is_alpha") => {
                Value::Bool(!s.is_empty() && s.chars().all(char::is_alphabetic))
            }
            (Value::Str(s), "is_blank") => Value::Bool(s.chars().all(char::is_whitespace)),
            (Value::Str(s), "code") => {
                let mut cs = s.chars();
                match (cs.next(), cs.next()) {
                    (Some(c), None) => Value::Int(c as i64),
                    _ => {
                        return self.fail(
                            "R024",
                            "`code()` needs a string of exactly one character",
                            span,
                        );
                    }
                }
            }
            (Value::Str(_), "to_int") => self.parse_number("int", recv)?,
            (Value::Str(_), "to_float") => self.parse_number("float", recv)?,
            (Value::Int(n), "char") => match u32::try_from(*n).ok().and_then(char::from_u32) {
                Some(c) => Value::str(&c.to_string()),
                None => {
                    return self.fail("R024", format!("{n} is not a valid character code"), span);
                }
            },
            (Value::Str(s), "upper") => Value::str(&s.to_uppercase()),
            (Value::Str(s), "lower") => Value::str(&s.to_lowercase()),
            (Value::Str(s), "trim") => Value::str(s.trim()),
            (Value::Str(s), "contains") => Value::Bool(s.contains(str_arg(0).unwrap().as_ref())),
            (Value::Str(s), "starts_with") => {
                Value::Bool(s.starts_with(str_arg(0).unwrap().as_ref()))
            }
            (Value::Str(s), "ends_with") => Value::Bool(s.ends_with(str_arg(0).unwrap().as_ref())),
            (Value::Str(s), "strip_prefix") => match s.strip_prefix(str_arg(0).unwrap().as_ref()) {
                Some(r) => Value::str(r),
                None => Value::None,
            },
            (Value::Str(s), "strip_suffix") => match s.strip_suffix(str_arg(0).unwrap().as_ref()) {
                Some(r) => Value::str(r),
                None => Value::None,
            },
            (Value::Str(s), "snake_case") => Value::str(&split_words(s).join("_").to_lowercase()),
            (Value::Str(s), "kebab_case") => Value::str(&split_words(s).join("-").to_lowercase()),
            (Value::Str(s), "camel_case") => {
                let words = split_words(s);
                let mut out = String::new();
                for (i, w) in words.iter().enumerate() {
                    if i == 0 {
                        out.push_str(&w.to_lowercase())
                    } else {
                        out.push_str(&capitalize_word(w))
                    }
                }
                Value::str(&out)
            }
            (Value::Str(s), "title_case") => {
                let words: Vec<String> =
                    split_words(s).iter().map(|w| capitalize_word(w)).collect();
                Value::str(&words.join(" "))
            }
            (Value::Str(s), "eq_ignore_case") => {
                Value::Bool(s.to_lowercase() == str_arg(0).unwrap().to_lowercase())
            }
            (Value::Str(s), "index_of") => {
                let needle = str_arg(0).unwrap();
                match s.find(needle.as_ref()) {
                    Some(b) => Value::Int(s[..b].chars().count() as i64),
                    None => Value::None,
                }
            }
            (Value::Str(s), "replace") => {
                let r = s.replace(str_arg(0).unwrap().as_ref(), str_arg(1).unwrap().as_ref());
                self.guard_alloc(r.len(), span)?;
                Value::str(&r)
            }
            (Value::Str(s), "split") => {
                let sep = str_arg(0).unwrap();
                let parts: Vec<Value> = if sep.is_empty() {
                    s.chars().map(|c| Value::str(&c.to_string())).collect()
                } else {
                    s.split(sep.as_ref()).map(Value::str).collect()
                };
                Value::array(parts)
            }
            (Value::Str(s), "repeat") => match int_arg(0).and_then(|n| usize::try_from(n).ok()) {
                Some(n) => {
                    self.guard_alloc(n.saturating_mul(s.len()), span)?;
                    Value::str(&s.repeat(n))
                }
                None => return self.fail("R021", "invalid repeat count", span),
            },
            (Value::Str(s), "chars") => {
                Value::array(s.chars().map(|c| Value::str(&c.to_string())).collect())
            }
            (Value::Str(s), "substring") => {
                let n = s.chars().count();
                let (a, b) = (int_arg(0).unwrap_or(-1), int_arg(1).unwrap_or(-1));
                if a < 0 || b < a || b as usize > n {
                    return self.fail(
                        "R020",
                        format!("invalid substring range {a}..{b} (length {n})"),
                        span,
                    );
                }
                Value::str(
                    &s.chars()
                        .skip(a as usize)
                        .take((b - a) as usize)
                        .collect::<String>(),
                )
            }
            (Value::Map(m), "len") => Value::Int(m.borrow().len() as i64),
            (Value::Map(m), "is_empty") => Value::Bool(m.borrow().is_empty()),
            (Value::Map(m), "clear") => {
                m.borrow_mut().clear();
                Value::Unit
            }
            (Value::Map(m), "has") => Value::Bool(m.borrow().get(&args[0]).is_some()),
            (Value::Map(m), "get") => m
                .borrow()
                .get(&args[0])
                .cloned()
                .unwrap_or_else(|| args[1].clone()),
            (Value::Map(m), "remove") => m.borrow_mut().remove(&args[0]).unwrap_or(Value::None),
            (Value::Map(m), "keys") => {
                Value::array(m.borrow().entries.iter().map(|(k, _)| k.clone()).collect())
            }
            (Value::Map(m), "values") => {
                Value::array(m.borrow().entries.iter().map(|(_, v)| v.clone()).collect())
            }
            (Value::Map(m), "merge") => {
                let Value::Map(other) = &args[0] else {
                    unreachable!("checked by the type checker")
                };
                self.charge(m.borrow().len() + other.borrow().len(), span)?;
                let mut out = MapObj::default();
                for (k, v) in m.borrow().entries.iter() {
                    out.insert(k.clone(), v.clone());
                }
                for (k, v) in other.borrow().entries.iter() {
                    out.insert(k.clone(), v.clone());
                }
                self.guard_alloc(out.len(), span)?;
                Value::Map(Rc::new(RefCell::new(out)))
            }
            (Value::Map(m), "filter") => {
                let entries: Vec<(Value, Value)> = m.borrow().entries.clone();
                let mut out = MapObj::default();
                for (k, v) in entries {
                    if matches!(
                        self.call_value(&args[0], vec![k.clone(), v.clone()], span)?,
                        Value::Bool(true)
                    ) {
                        out.insert(k, v);
                    }
                }
                Value::Map(Rc::new(RefCell::new(out)))
            }
            (Value::Map(m), "map_values") => {
                let entries: Vec<(Value, Value)> = m.borrow().entries.clone();
                let mut out = MapObj::default();
                for (k, v) in entries {
                    let nv = self.call_value(&args[0], vec![k.clone(), v], span)?;
                    out.insert(k, nv);
                }
                Value::Map(Rc::new(RefCell::new(out)))
            }
            (Value::Array(a), "sort") => {
                let items: Vec<Value> = a.borrow().clone();
                let sorted = self.merge_sort(items, span)?;
                *a.borrow_mut() = sorted;
                Value::Unit
            }
            (Value::Array(a), "min" | "max") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut best: Option<Value> = None;
                for it in items {
                    best = Some(match best {
                        None => it,
                        Some(b) => {
                            let o = self.order(&it, &b, span)?;
                            if (name == "min" && o == Ordering::Less)
                                || (name == "max" && o == Ordering::Greater)
                            {
                                it
                            } else {
                                b
                            }
                        }
                    });
                }
                best.unwrap_or(Value::None)
            }
            (Value::Array(a), "sum") => {
                let items: Vec<Value> = a.borrow().clone();
                let all_int = items.iter().all(|v| matches!(v, Value::Int(_)));
                if items.is_empty() {
                    if self.prog.res.float_sums.contains(&self.cur_call) {
                        Value::Float(0.0)
                    } else {
                        Value::Int(0)
                    }
                } else if all_int {
                    let mut total: i64 = 0;
                    for v in &items {
                        if let Value::Int(n) = v {
                            total = match total.checked_add(*n) {
                                Some(t) => t,
                                None => return self.fail("R010", "integer overflow", span),
                            };
                        }
                    }
                    Value::Int(total)
                } else {
                    Value::Float(items.iter().filter_map(num).sum())
                }
            }
            (Value::Array(a), "slice") => {
                let len = a.borrow().len();
                let (s, e) = (int_arg(0).unwrap_or(-1), int_arg(1).unwrap_or(-1));
                if s < 0 || e < s || e as usize > len {
                    return self.fail(
                        "R020",
                        format!("invalid slice {s}..{e} (length {len})"),
                        span,
                    );
                }
                Value::array(a.borrow()[s as usize..e as usize].to_vec())
            }
            (Value::Array(a), "map") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.call_value(&args[0], vec![it], span)?);
                }
                self.guard_alloc(out.len(), span)?;
                Value::array(out)
            }
            (Value::Array(a), "filter") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut out = Vec::new();
                for it in items {
                    if matches!(
                        self.call_value(&args[0], vec![it.clone()], span)?,
                        Value::Bool(true)
                    ) {
                        out.push(it);
                    }
                }
                Value::array(out)
            }
            (Value::Array(a), "reduce") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut acc = args[0].clone();
                for it in items {
                    acc = self.call_value(&args[1], vec![acc, it], span)?;
                }
                acc
            }
            (Value::Array(a), "any" | "all") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut result = name == "all";
                for it in items {
                    let ok = matches!(
                        self.call_value(&args[0], vec![it], span)?,
                        Value::Bool(true)
                    );
                    if name == "any" && ok {
                        result = true;
                        break;
                    }
                    if name == "all" && !ok {
                        result = false;
                        break;
                    }
                }
                Value::Bool(result)
            }
            (Value::Array(a), "find") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut found = Value::None;
                for it in items {
                    if matches!(
                        self.call_value(&args[0], vec![it.clone()], span)?,
                        Value::Bool(true)
                    ) {
                        found = it;
                        break;
                    }
                }
                found
            }
            (Value::Array(a), "each") => {
                let items: Vec<Value> = a.borrow().clone();
                for it in items {
                    self.call_value(&args[0], vec![it], span)?;
                }
                Value::Unit
            }
            (Value::Array(a), "sort_by") => {
                let items: Vec<Value> = a.borrow().clone();
                let sorted = self.merge_sort_by(items, &args[0], span)?;
                *a.borrow_mut() = sorted;
                Value::Unit
            }
            (Value::Array(a), "unique") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut out: Vec<Value> = Vec::new();
                for it in items {
                    let mut dup = false;
                    for existing in &out {
                        if self.values_equal(existing, &it, span)? {
                            dup = true;
                            break;
                        }
                    }
                    if !dup {
                        out.push(it);
                    }
                }
                Value::array(out)
            }
            (Value::Array(a), "first") => a.borrow().first().cloned().unwrap_or(Value::None),
            (Value::Array(a), "last") => a.borrow().last().cloned().unwrap_or(Value::None),
            (Value::Array(a), "concat") => {
                let Value::Array(other) = &args[0] else {
                    unreachable!("checked by the type checker")
                };
                self.charge(other.borrow().len(), span)?;
                let mut out = a.borrow().clone();
                out.extend(other.borrow().iter().cloned());
                self.guard_alloc(out.len(), span)?;
                Value::array(out)
            }
            (Value::Array(a), "take") => {
                let n = int_arg(0).unwrap_or(0).max(0) as usize;
                Value::array(a.borrow().iter().take(n).cloned().collect())
            }
            (Value::Array(a), "drop") => {
                let n = int_arg(0).unwrap_or(0).max(0) as usize;
                Value::array(a.borrow().iter().skip(n).cloned().collect())
            }
            (Value::Array(a), "swap") => {
                let len = a.borrow().len();
                match (
                    int_arg(0).and_then(|i| usize::try_from(i).ok()),
                    int_arg(1).and_then(|i| usize::try_from(i).ok()),
                ) {
                    (Some(i), Some(j)) if i < len && j < len => {
                        a.borrow_mut().swap(i, j);
                        Value::Unit
                    }
                    _ => {
                        return self.fail(
                            "R020",
                            format!("cannot swap these indices (length {len})"),
                            span,
                        );
                    }
                }
            }
            (Value::Array(a), "chunk") => {
                let n = match int_arg(0) {
                    Some(n) if n > 0 => n as usize,
                    _ => return self.fail("R021", "the chunk size must be a positive int", span),
                };
                let items: Vec<Value> = a.borrow().clone();
                Value::array(items.chunks(n).map(|c| Value::array(c.to_vec())).collect())
            }
            (Value::Array(a), "repeat") => {
                let n = int_arg(0).unwrap_or(0).max(0) as usize;
                let items: Vec<Value> = a.borrow().clone();
                let total = items.len().saturating_mul(n);
                self.guard_alloc(total, span)?;
                self.charge(total, span)?;
                Value::array(items.iter().cloned().cycle().take(total).collect())
            }
            (Value::Array(a), "flatten") => {
                let items: Vec<Value> = a.borrow().clone();
                let mut out = Vec::new();
                for it in items {
                    match it {
                        Value::Array(inner) => out.extend(inner.borrow().iter().cloned()),
                        other => out.push(other),
                    }
                }
                self.guard_alloc(out.len(), span)?;
                Value::array(out)
            }
            (Value::Int(x), "min" | "max") => {
                let y = int_arg(0).unwrap_or(0);
                Value::Int(if name == "min" {
                    (*x).min(y)
                } else {
                    (*x).max(y)
                })
            }
            (Value::Int(_), "pow") => match self.arith(BinOp::Pow, recv, &args[0], span)? {
                Some(v) => v,
                None => return Ok(None),
            },
            (Value::Int(x), "sqrt") => {
                if *x < 0 {
                    return self.fail("R014", "square root of a negative number", span);
                }
                Value::Float((*x as f64).sqrt())
            }
            (Value::Float(x), "min" | "max") => {
                let y = num(&args[0]).unwrap_or(0.0);
                Value::Float(if name == "min" { x.min(y) } else { x.max(y) })
            }
            (Value::Float(x), "pow") => Value::Float(x.powf(num(&args[0]).unwrap_or(0.0))),
            (Value::Int(i), "abs") => match i.checked_abs() {
                Some(v) => Value::Int(v),
                None => return self.fail("R010", "integer overflow", span),
            },
            (Value::Int(n), "to_hex") => Value::str(&if *n < 0 {
                format!("-{:x}", n.unsigned_abs())
            } else {
                format!("{n:x}")
            }),
            (Value::Int(n), "to_oct") => Value::str(&if *n < 0 {
                format!("-{:o}", n.unsigned_abs())
            } else {
                format!("{n:o}")
            }),
            (Value::Int(n), "to_bin") => Value::str(&if *n < 0 {
                format!("-{:b}", n.unsigned_abs())
            } else {
                format!("{n:b}")
            }),
            (Value::Float(f), "to_fixed") => match int_arg(0) {
                Some(n) if (0..=17).contains(&n) => Value::str(&format!("{:.*}", n as usize, f)),
                _ => {
                    return self.fail(
                        "R021",
                        "the number of decimals must be between 0 and 17",
                        span,
                    );
                }
            },
            (Value::Float(f), "abs") => Value::Float(f.abs()),
            (Value::Float(f), "sqrt") => {
                if *f < 0.0 {
                    return self.fail("R014", "square root of a negative number", span);
                }
                Value::Float(f.sqrt())
            }
            (
                Value::Int(_) | Value::Float(_),
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "ln" | "log10" | "log2"
                | "to_radians" | "to_degrees",
            ) => {
                let x = num(recv).unwrap_or(f64::NAN);
                let domain = match name {
                    "asin" | "acos" => (-1.0..=1.0).contains(&x),
                    "ln" | "log10" | "log2" => x > 0.0,
                    _ => true,
                };
                if !domain {
                    return self.fail(
                        "R016",
                        format!("{x} is outside the domain of `{name}`"),
                        span,
                    );
                }
                Value::Float(match name {
                    "sin" => x.sin(),
                    "cos" => x.cos(),
                    "tan" => x.tan(),
                    "asin" => x.asin(),
                    "acos" => x.acos(),
                    "atan" => x.atan(),
                    "exp" => x.exp(),
                    "ln" => x.ln(),
                    "log10" => x.log10(),
                    "log2" => x.log2(),
                    "to_radians" => x.to_radians(),
                    _ => x.to_degrees(),
                })
            }
            (Value::Int(_) | Value::Float(_), "log") => {
                let (x, base) = (
                    num(recv).unwrap_or(f64::NAN),
                    num(&args[0]).unwrap_or(f64::NAN),
                );
                if x <= 0.0 || base <= 0.0 || base == 1.0 {
                    return self.fail(
                        "R016",
                        format!("`log` needs a positive number and a positive base other than 1 (found {x} and {base})"),
                        span,
                    );
                }
                Value::Float(x.ln() / base.ln())
            }
            (Value::Int(_) | Value::Float(_), "atan2") => Value::Float(
                num(recv)
                    .unwrap_or(f64::NAN)
                    .atan2(num(&args[0]).unwrap_or(f64::NAN)),
            ),
            (Value::Int(x), "signum") => Value::Int(x.signum()),
            (Value::Float(x), "signum") => Value::Float(if x.is_nan() {
                *x
            } else if *x == 0.0 {
                0.0
            } else {
                x.signum()
            }),
            (Value::Int(x), "clamp") => {
                let (lo, hi) = (int_arg(0).unwrap_or(0), int_arg(1).unwrap_or(0));
                if lo > hi {
                    return self.fail(
                        "R016",
                        format!("`clamp` needs min <= max (found {lo} and {hi})"),
                        span,
                    );
                }
                Value::Int((*x).clamp(lo, hi))
            }
            (Value::Float(x), "clamp") => {
                let (lo, hi) = (num(&args[0]).unwrap_or(0.0), num(&args[1]).unwrap_or(0.0));
                if lo.is_nan() || hi.is_nan() || lo > hi {
                    return self.fail(
                        "R016",
                        format!("`clamp` needs min <= max (found {lo} and {hi})"),
                        span,
                    );
                }
                Value::Float(x.clamp(lo, hi))
            }
            (Value::Float(x), "is_nan") => Value::Bool(x.is_nan()),
            (Value::Float(x), "is_infinite") => Value::Bool(x.is_infinite()),
            (Value::Float(f), "floor" | "ceil" | "round" | "trunc") => {
                let r = match name {
                    "floor" => f.floor(),
                    "ceil" => f.ceil(),
                    "trunc" => f.trunc(),
                    _ => f.round(),
                };
                if !r.is_finite() || r.abs() >= 9.3e18 {
                    return self.fail("R013", format!("cannot convert {f} to an integer"), span);
                }
                Value::Int(r as i64)
            }
            _ => return Ok(None),
        }))
    }
}

/// Splits text into words for case conversion: separators are any non-alphanumeric
/// character, plus a boundary before an upper-case letter following a lower-case
/// letter or digit (so `camelCase` and `snake_case` both split cleanly).
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c.is_uppercase()
            && let Some(prev) = cur.chars().last()
            && (prev.is_lowercase() || prev.is_numeric())
        {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn capitalize_word(w: &str) -> String {
    let mut cs = w.chars();
    match cs.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn ord_matches(op: BinOp, ord: Option<Ordering>) -> bool {
    match ord {
        None => false,
        Some(o) => match op {
            BinOp::Lt => o == Ordering::Less,
            BinOp::LtEq => o != Ordering::Greater,
            BinOp::Gt => o == Ordering::Greater,
            BinOp::GtEq => o != Ordering::Less,
            _ => false,
        },
    }
}

fn fmt_float(f: f64) -> String {
    if f.is_nan() {
        "nan".to_string()
    } else if f.is_infinite() {
        if f > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        }
    } else {
        format!("{f:?}")
    }
}
