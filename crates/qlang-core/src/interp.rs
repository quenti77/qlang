//! Interpreter: runs a checked [`Program`].
//!
//! Everything that reaches outside goes through the [`Host`]. Execution is
//! bounded by [`Limits`] so that a runaway program cannot hang its host.

use crate::ast::*;
use crate::check::Program;
use crate::diag::Diagnostic;
use crate::host::{Host, HostError, Limits};
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
    let mut it = Interp {
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
    };
    let error = match it.run_all() {
        Ok(()) => None,
        Err(Ctrl::Error(mut d)) => {
            d.resolve(&prog.sources);
            Some(*d)
        }
        Err(_) => None,
    };
    RunResult { error, steps: it.steps }
}

enum Ctrl {
    Break,
    Continue,
    Return(Value),
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
    frames: Vec<(String, Span)>,
    /// The call expression being evaluated (for `sum()` on empty float arrays).
    cur_call: NodeId,
}

impl<'a> Interp<'a> {
    // ------------------------------------------------------------------ errors

    fn error(&self, code: &str, msg: impl Into<String>, span: Span) -> Ctrl {
        let mut d = Diagnostic::error(code, msg, span);
        // innermost call first; identical consecutive calls (recursion) are merged
        let mut groups: Vec<(&String, Span, usize)> = Vec::new();
        for (name, call_span) in self.frames.iter().rev() {
            match groups.last_mut() {
                Some((n, s, count)) if *n == name && *s == *call_span => *count += 1,
                _ => groups.push((name, *call_span, 1)),
            }
        }
        for (name, call_span, count) in groups.into_iter().take(8) {
            let times = if count > 1 { format!(" ({count} times)") } else { String::new() };
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
            return self.fail("R900", "the program ran for too long (step limit reached: infinite loop?)", span);
        }
        Ok(())
    }

    /// Refuse arrays and strings that grow beyond the configured size.
    fn guard_alloc(&self, len: usize, span: Span) -> R<()> {
        if len > self.limits.max_alloc {
            return self.fail("R903", "a value became too large (memory limit reached)", span);
        }
        Ok(())
    }

    fn emit(&mut self, text: &str, span: Span) -> R<()> {
        self.out_bytes += text.len();
        if self.out_bytes > self.limits.max_output {
            return self.fail("R902", "the program printed too much (output limit reached)", span);
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
            StmtKind::While { cond, body } => {
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
            StmtKind::For { var, var2, iter, step, body } => {
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
        }
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
            ExprKind::Str(parts) => {
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
                Ok(Value::str(&out))
            }
            ExprKind::Ident(name, _) => match prog.res.paths.get(&e.id) {
                Some(PathRes::Global(m, n)) => match self.envs[*m].get(n) {
                    Some(v) => Ok(v),
                    None => self.fail("R002", format!("`{n}` is used before it has a value"), e.span),
                },
                Some(PathRes::Fn(fid)) => Ok(self.fn_value(*fid)),
                _ => match env.get(name) {
                    Some(v) => Ok(v),
                    None => self.fail("R002", format!("`{name}` is used before it has a value"), e.span),
                },
            },
            ExprKind::SelfVal => match env.get("self") {
                Some(v) => Ok(v),
                None => self.fail("R002", "`self` is not available here", e.span),
            },
            ExprKind::Super => self.fail("R003", "`super` can only be used for calls", e.span),
            ExprKind::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval(it, env)?);
                }
                Ok(Value::array(out))
            }
            ExprKind::Map(items) => {
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
            ExprKind::StructLit(lit) => self.eval_struct_lit(e, lit, env),
            ExprKind::Unary(op, inner) => {
                let v = self.eval(inner, env)?;
                match (op, &v) {
                    (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnOp::Neg, Value::Int(i)) => match i.checked_neg() {
                        Some(n) => Ok(Value::Int(n)),
                        None => self.fail("R010", "integer overflow", e.span),
                    },
                    (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
                    (UnOp::Neg, _) => match self.find_method(&v, Lang::Neg.method_name(), Some(prog.defs.lang(Lang::Neg)), None) {
                        Some(fid) => self.call_fid(fid, Some(v), vec![], e.span),
                        None => self.fail("R004", "this value cannot be negated", e.span),
                    },
                    _ => self.fail("R001", "unexpected operand", e.span),
                }
            }
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
            ExprKind::Range { start, end, inclusive } => {
                let (a, b) = (self.eval(start, env)?, self.eval(end, env)?);
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Range(a, b, *inclusive)),
                    _ => self.fail("R001", "a range needs integer bounds", e.span),
                }
            }
            ExprKind::Cast(inner, _) => {
                let v = self.eval(inner, env)?;
                let target = prog.res.casts.get(&e.id).cloned().unwrap_or(Ty::Error);
                self.convert(v, &target, e.span)
            }
            ExprKind::Call { callee, args } => self.eval_call(e, callee, args, env),
            ExprKind::Index(base, idx) => {
                let b = self.eval(base, env)?;
                let i = self.eval(idx, env)?;
                self.index_get(&b, &i, e.span)
            }
            ExprKind::Append(_) => self.fail("R003", "`x[]` can only be assigned to", e.span),
            ExprKind::Field(obj, name, _) => self.eval_field(e, obj, name, env),
            ExprKind::If { branches, else_block } => {
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
            ExprKind::Match { scrutinee, cases, else_block } => {
                let v = self.eval(scrutinee, env)?;
                for (i, case) in cases.iter().enumerate() {
                    let scope = Scope::new(Some(env.clone()));
                    if !self.match_pattern(&case.pattern, &v, e.id, i, &scope)? {
                        continue;
                    }
                    if let Some(g) = &case.guard
                        && !self.eval_bool(g, &scope)? {
                            continue;
                        }
                    return self.exec_stmts(&case.body.stmts, &scope);
                }
                match else_block {
                    Some(b) => self.exec_block(b, env),
                    None => Ok(Value::Unit),
                }
            }
            ExprKind::Lambda(f) => Ok(Value::Func(Rc::new(FuncVal { decl: f.clone(), env: env.clone(), name: "<function>".into() }))),
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
        let mut fields: HashMap<String, Value> = HashMap::new();
        if let Some(b) = &lit.base {
            match self.eval(b, env)? {
                Value::Struct(o) => {
                    for (k, v) in o.fields.borrow().iter() {
                        fields.insert(k.clone(), v.clone());
                    }
                }
                _ => return self.fail("R001", "`..base` must be a struct value", b.span),
            }
        }
        for item in &lit.fields {
            let v = self.eval(&item.value, env)?;
            fields.insert(item.name.name.clone(), v);
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
                    fields.insert(f.name.clone(), v);
                }
            }
        }
        Ok(Value::Struct(Rc::new(StructObj { def: sid, fields: RefCell::new(fields) })))
    }

    fn eval_field(&mut self, e: &Expr, obj: &Expr, name: &Ident, env: &Env) -> R<Value> {
        let prog = self.prog;
        match prog.res.paths.get(&e.id) {
            Some(PathRes::EnumVariant(eid, vi)) => return Ok(Value::Enum(*eid, *vi)),
            Some(PathRes::StaticField(sid, n)) => {
                return match self.statics.get(&(*sid, n.clone())) {
                    Some(v) => Ok(v.clone()),
                    None => self.fail("R002", format!("static field `{n}` has no value yet"), e.span),
                };
            }
            Some(PathRes::BuiltinStatic(ty, name)) => {
                return Ok(match (*ty, *name) {
                    ("float", "PI") => Value::Float(std::f64::consts::PI),
                    ("float", "E") => Value::Float(std::f64::consts::E),
                    ("int", "MAX") => Value::Int(i64::MAX),
                    ("int", "MIN") => Value::Int(i64::MIN),
                    _ => Value::Unit,
                });
            }
            Some(PathRes::Fn(fid)) => return Ok(self.fn_value(*fid)),
            Some(PathRes::Global(m, n)) => {
                return match self.envs[*m].get(n) {
                    Some(v) => Ok(v),
                    None => self.fail("R002", format!("`{n}` is used before it has a value"), e.span),
                };
            }
            _ => {}
        }
        match self.eval(obj, env)? {
            Value::Struct(o) => {
                let v = o.fields.borrow().get(&name.name).cloned();
                match v {
                    Some(v) => Ok(v),
                    None => self.fail("R005", format!("this value has no field `{}`", name.name), name.span),
                }
            }
            Value::None => self.fail("R006", format!("cannot read `.{}`: the value is `none`", name.name), e.span),
            _ => self.fail("R005", format!("this value has no field `{}`", name.name), name.span),
        }
    }

    // ----------------------------------------------------------------- calls

    fn eval_args(&mut self, args: &[Expr], env: &Env) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(a, env)?);
        }
        Ok(out)
    }

    fn eval_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr], env: &Env) -> R<Value> {
        let prog = self.prog;
        match &callee.kind {
            ExprKind::Ident(..) | ExprKind::Field(..) => {
                match prog.res.paths.get(&callee.id) {
                    Some(PathRes::Fn(fid)) => {
                        let argv = self.eval_args(args, env)?;
                        return self.call_fid(*fid, None, argv, e.span);
                    }
                    Some(PathRes::BuiltinFn(name)) => {
                        let argv = self.eval_args(args, env)?;
                        return self.builtin_fn(name, argv, e.span);
                    }
                    Some(PathRes::BuiltinStatic(ty, "parse")) => {
                        let argv = self.eval_args(args, env)?;
                        return self.parse_number(ty, &argv[0]);
                    }
                    _ => {}
                }
                if let ExprKind::Field(obj, name, _) = &callee.kind {
                    if let Some(fid) = prog.res.super_calls.get(&callee.id) {
                        let this = env.get("self");
                        let argv = self.eval_args(args, env)?;
                        return self.call_fid(*fid, this, argv, e.span);
                    }
                    // a module member that is a variable holding a function, or a method call
                    if !matches!(prog.res.paths.get(&obj.id), Some(PathRes::Module(_)) | Some(PathRes::Type(_))) {
                        let recv = self.eval(obj, env)?;
                        let argv = self.eval_args(args, env)?;
                        self.cur_call = e.id;
                        return self.call_method(recv, name, argv, e.span);
                    }
                }
            }
            _ => {}
        }
        let f = self.eval(callee, env)?;
        let argv = self.eval_args(args, env)?;
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

    fn call_fid(&mut self, fid: FnId, this: Option<Value>, args: Vec<Value>, span: Span) -> R<Value> {
        let prog = self.prog;
        let def = &prog.defs.fns[fid];
        let decl = def.decl.clone().expect("function without body");
        let env = self.envs[def.module].clone();
        self.call_function(&decl, &env, this, args, &def.name, span)
    }

    fn call_function(&mut self, decl: &Rc<FunDecl>, parent: &Env, this: Option<Value>, args: Vec<Value>, name: &str, span: Span) -> R<Value> {
        if self.depth >= self.limits.max_depth {
            return self.fail("R901", "too many nested calls (infinite recursion?)", span);
        }
        self.depth += 1;
        self.frames.push((name.to_string(), span));
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
            return self.fail("R006", format!("cannot call `.{}()`: the value is `none`", name.name), span);
        }
        if !matches!(recv, Value::Struct(_) | Value::Enum(..))
            && let Some(v) = self.builtin_method(&recv, &name.name, &args, span)? {
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
        self.fail("R008", format!("this value has no method `{}`", name.name), span)
    }

    // ---------------------------------------------------------- method lookup

    fn value_matches_ty(&self, v: &Value, t: &Ty) -> bool {
        let defs = &self.prog.defs;
        match (v, t) {
            (_, Ty::Error | Ty::Param(_) | Ty::Infer(_)) => true,
            (Value::Int(_), Ty::Int) | (Value::Float(_), Ty::Float) | (Value::Bool(_), Ty::Bool) => true,
            (Value::Str(_), Ty::Str) | (Value::Range(..), Ty::Range) => true,
            (Value::None, Ty::NoneT | Ty::Nullable(_)) => true,
            (_, Ty::Nullable(inner)) => self.value_matches_ty(v, inner),
            (Value::Array(_), Ty::Array(_)) => true,
            (Value::Map(_), Ty::Map(..)) => true,
            (Value::Struct(o), Ty::Struct(sid, _)) => defs.is_subclass(o.def, *sid),
            (Value::Struct(o), Ty::Trait(tid, _)) => {
                let classes: Vec<StructId> = defs.ancestry(o.def, &[]).into_iter().map(|(s, _)| s).collect();
                defs.impls.iter().any(|imp| {
                    matches!(&imp.trait_, Some((t2, _)) if t2 == tid) && matches!(&imp.target, Ty::Struct(s, _) if classes.contains(s))
                })
            }
            (Value::Enum(e, _), Ty::Enum(x)) => e == x,
            (Value::Func(_), Ty::Fn(..)) => true,
            _ => false,
        }
    }

    /// The impl method `name` for a value, optionally restricted to a trait and
    /// to the impl whose first parameter accepts `arg`.
    fn find_method(&self, recv: &Value, name: &str, tid: Option<TraitId>, arg: Option<&Value>) -> Option<FnId> {
        let defs = &self.prog.defs;
        let classes: Vec<Option<StructId>> = match recv {
            Value::Struct(o) => defs.ancestry(o.def, &[]).into_iter().map(|(s, _)| Some(s)).collect(),
            _ => vec![None],
        };
        for cls in classes {
            for pass in 0..2 {
                for imp in &defs.impls {
                    if (pass == 0) != imp.trait_.is_none() {
                        continue;
                    }
                    if let Some(t) = tid
                        && !matches!(&imp.trait_, Some((x, _)) if *x == t) {
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
                    let Some((_, fid)) = imp.methods.iter().find(|(n, _)| n == name) else { continue };
                    if let Some(a) = arg
                        && let Some(p0) = defs.fns[*fid].params.first()
                            && !self.value_matches_ty(a, p0) {
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
            Value::Struct(o) => defs.ancestry(o.def, &[]).into_iter().map(|(s, _)| Some(s)).collect(),
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
                    return imp.methods.iter().find(|(n, _)| n == "convert").map(|(_, f)| *f);
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
        Ok(match v {
            Value::Str(s) => s.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => fmt_float(*f),
            Value::Bool(b) => b.to_string(),
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
            Value::Enum(e, vi) => {
                let d = &self.prog.defs.enums[*e];
                format!("{}.{}", d.name, d.variants[*vi])
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
                    if parts.is_empty() { name.clone() } else { format!("{name} {{ {} }}", parts.join(", ")) }
                }
            },
        })
    }

    /// Like `stringify`, but strings are quoted (used inside arrays and structs).
    fn repr(&mut self, v: &Value, span: Span) -> R<String> {
        if let Value::Str(s) = v {
            let esc = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t");
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
                        if let (Value::Int(x), Value::Int(y)) = (&l, &r) { Some(x.cmp(y)) } else { None }
                    } else {
                        a.partial_cmp(&b)
                    };
                    return Ok(Value::Bool(ord_matches(op, ord)));
                }
                let ord = match (&l, &r) {
                    (Value::Str(a), Value::Str(b)) => Some(a.as_ref().cmp(b.as_ref())),
                    _ => match self.find_method(&l, Lang::Ord.method_name(), Some(prog.defs.lang(Lang::Ord)), Some(&r)) {
                        Some(fid) => match self.call_fid(fid, Some(l.clone()), vec![r.clone()], span)? {
                            Value::Int(i) => Some(i.cmp(&0)),
                            _ => return self.fail("R001", "`cmp` must return an int", span),
                        },
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
            None => self.fail("R004", format!("cannot apply `{}` to these values", op.symbol()), span),
        }
    }

    fn arith(&mut self, op: BinOp, l: &Value, r: &Value, span: Span) -> R<Option<Value>> {
        match (l, r) {
            (Value::Int(a), Value::Int(b)) => {
                let (a, b) = (*a, *b);
                let overflow = |this: &Self| this.fail::<Option<Value>>("R010", "integer overflow", span);
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
                            return self.fail("R012", "negative exponent: use decimals (`2.0 ** -1`) for fractions", span);
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
            (Value::Str(a), Value::Str(b)) if op == BinOp::Add => {
                self.guard_alloc(a.len() + b.len(), span)?;
                let mut s = String::with_capacity(a.len() + b.len());
                s.push_str(a);
                s.push_str(b);
                Ok(Some(Value::str(&s)))
            }
            _ => {
                let (Some(a), Some(b)) = (num(l), num(r)) else { return Ok(None) };
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
            (Value::Enum(e1, v1), Value::Enum(e2, v2)) => e1 == e2 && v1 == v2,
            (Value::Map(x), Value::Map(y)) => Rc::ptr_eq(x, y),
            (Value::Array(x), Value::Array(y)) => {
                if Rc::ptr_eq(x, y) {
                    return Ok(true);
                }
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
                match self.find_method(a, Lang::Eq.method_name(), Some(prog.defs.lang(Lang::Eq)), Some(b)) {
                    Some(fid) => match self.call_fid(fid, Some(a.clone()), vec![b.clone()], span)? {
                        Value::Bool(r) => r,
                        _ => return self.fail("R001", "`eq` must return a bool", span),
                    },
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
                    None => self.fail("R020", format!("index {i} is out of bounds (length {})", a.len()), span),
                }
            }
            (Value::Map(m), k) => Ok(m.borrow().get(k).cloned().unwrap_or(Value::None)),
            (Value::Str(s), Value::Int(i)) => match usize::try_from(*i).ok().and_then(|u| s.chars().nth(u)) {
                Some(c) => Ok(Value::str(&c.to_string())),
                None => self.fail("R020", format!("index {i} is out of bounds (length {})", s.chars().count()), span),
            },
            _ => match self.find_method(base, Lang::Index.method_name(), Some(self.prog.defs.lang(Lang::Index)), Some(idx)) {
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
                    Value::None => self.fail("R006", format!("cannot set `.{}`: the value is `none`", name.name), target.span),
                    _ => self.fail("R005", format!("this value has no field `{}`", name.name), name.span),
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
                            None => self.fail("R020", format!("index {n} is out of bounds (length {len})"), target.span),
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
                o.fields.borrow_mut().insert(n.clone(), v);
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
                    let Value::Int(n) = i else { return self.fail("R020", "an array index must be an int", span) };
                    let len = a.borrow().len();
                    match usize::try_from(*n).ok().filter(|u| *u < len) {
                        Some(u) => a.borrow_mut()[u] = v,
                        None => return self.fail("R020", format!("index {n} is out of bounds (length {len})"), span),
                    }
                }
                _ => match self.find_method(b, Lang::IndexSet.method_name(), Some(prog.defs.lang(Lang::IndexSet)), Some(i)) {
                    Some(fid) => {
                        self.call_fid(fid, Some(b.clone()), vec![i.clone(), v], span)?;
                    }
                    None => return self.fail("R004", "this value cannot be assigned by index", span),
                },
            },
            Place::Append(b) => match b {
                Value::Array(a) => {
                    self.guard_alloc(a.borrow().len() + 1, span)?;
                    a.borrow_mut().push(v)
                }
                _ => match self.find_method(b, Lang::Push.method_name(), Some(prog.defs.lang(Lang::Push)), Some(&v)) {
                    Some(fid) => {
                        self.call_fid(fid, Some(b.clone()), vec![v], span)?;
                    }
                    None => return self.fail("R004", "this value cannot be appended to", span),
                },
            },
        }
        Ok(())
    }

    fn eval_assign(&mut self, e: &Expr, op: Option<BinOp>, target: &Expr, value: &Expr, env: &Env) -> R<Value> {
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

    fn match_pattern(&mut self, p: &Pattern, v: &Value, mid: NodeId, idx: usize, scope: &Env) -> R<bool> {
        Ok(match p {
            Pattern::Int(n) => matches!(v, Value::Int(x) if x == n) || matches!(v, Value::Float(f) if *f == *n as f64),
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
                let (Some(x), l, h) = (num(v), bound(lo), bound(hi)) else { return Ok(false) };
                x >= l && if *inclusive { x <= h } else { x < h }
            }
            Pattern::Path(_) => match (self.prog.res.patterns.get(&(mid, idx)), v) {
                (Some((eid, vi)), Value::Enum(e, x)) => eid == e && vi == x,
                _ => false,
            },
            Pattern::Bind(name) => {
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
            _ => {
                if let (Some(x), Some(y)) = (num(a), num(b)) {
                    return Ok(x.total_cmp(&y));
                }
                let ord = self.prog.defs.lang(Lang::Ord);
                match self.find_method(a, Lang::Ord.method_name(), Some(ord), Some(b)) {
                    Some(fid) => match self.call_fid(fid, Some(a.clone()), vec![b.clone()], span)? {
                        Value::Int(i) => Ok(i.cmp(&0)),
                        _ => self.fail("R001", "`cmp` must return an int", span),
                    },
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
                Err(HostError::Unsupported(_)) => self.fail("R041", "`read` is not available in this environment", span),
                Err(HostError::Other(m)) => self.fail("R041", format!("cannot read input: {m}"), span),
            },
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

    fn parse_number(&mut self, ty: &str, v: &Value) -> R<Value> {
        let Value::Str(s) = v else { return Ok(Value::None) };
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

    fn builtin_method(&mut self, recv: &Value, name: &str, args: &[Value], span: Span) -> R<Option<Value>> {
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
                match int_arg(0).and_then(|i| usize::try_from(i).ok()).filter(|i| *i <= len) {
                    Some(i) => {
                        self.guard_alloc(len + 1, span)?;
                        a.borrow_mut().insert(i, args[1].clone());
                        Value::Unit
                    }
                    None => return self.fail("R020", format!("cannot insert at this index (length {len})"), span),
                }
            }
            (Value::Array(a), "remove") => {
                let len = a.borrow().len();
                match int_arg(0).and_then(|i| usize::try_from(i).ok()).filter(|i| *i < len) {
                    Some(i) => a.borrow_mut().remove(i),
                    None => return self.fail("R020", format!("cannot remove at this index (length {len})"), span),
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
            (Value::Str(s), "len") => Value::Int(s.chars().count() as i64),
            (Value::Str(s), "is_empty") => Value::Bool(s.is_empty()),
            (Value::Str(s), "upper") => Value::str(&s.to_uppercase()),
            (Value::Str(s), "lower") => Value::str(&s.to_lowercase()),
            (Value::Str(s), "trim") => Value::str(s.trim()),
            (Value::Str(s), "contains") => Value::Bool(s.contains(str_arg(0).unwrap().as_ref())),
            (Value::Str(s), "starts_with") => Value::Bool(s.starts_with(str_arg(0).unwrap().as_ref())),
            (Value::Str(s), "ends_with") => Value::Bool(s.ends_with(str_arg(0).unwrap().as_ref())),
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
            (Value::Str(s), "chars") => Value::array(s.chars().map(|c| Value::str(&c.to_string())).collect()),
            (Value::Str(s), "substring") => {
                let n = s.chars().count();
                let (a, b) = (int_arg(0).unwrap_or(-1), int_arg(1).unwrap_or(-1));
                if a < 0 || b < a || b as usize > n {
                    return self.fail("R020", format!("invalid substring range {a}..{b} (length {n})"), span);
                }
                Value::str(&s.chars().skip(a as usize).take((b - a) as usize).collect::<String>())
            }
            (Value::Map(m), "len") => Value::Int(m.borrow().len() as i64),
            (Value::Map(m), "is_empty") => Value::Bool(m.borrow().is_empty()),
            (Value::Map(m), "clear") => {
                m.borrow_mut().clear();
                Value::Unit
            }
            (Value::Map(m), "has") => Value::Bool(m.borrow().get(&args[0]).is_some()),
            (Value::Map(m), "get") => m.borrow().get(&args[0]).cloned().unwrap_or_else(|| args[1].clone()),
            (Value::Map(m), "remove") => m.borrow_mut().remove(&args[0]).unwrap_or(Value::None),
            (Value::Map(m), "keys") => Value::array(m.borrow().entries.iter().map(|(k, _)| k.clone()).collect()),
            (Value::Map(m), "values") => Value::array(m.borrow().entries.iter().map(|(_, v)| v.clone()).collect()),
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
                            if (name == "min" && o == Ordering::Less) || (name == "max" && o == Ordering::Greater) { it } else { b }
                        }
                    });
                }
                best.unwrap_or(Value::None)
            }
            (Value::Array(a), "sum") => {
                let items: Vec<Value> = a.borrow().clone();
                let all_int = items.iter().all(|v| matches!(v, Value::Int(_)));
                if items.is_empty() {
                    if self.prog.res.float_sums.contains(&self.cur_call) { Value::Float(0.0) } else { Value::Int(0) }
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
                    return self.fail("R020", format!("invalid slice {s}..{e} (length {len})"), span);
                }
                Value::array(a.borrow()[s as usize..e as usize].to_vec())
            }
            (Value::Int(x), "min" | "max") => {
                let y = int_arg(0).unwrap_or(0);
                Value::Int(if name == "min" { (*x).min(y) } else { (*x).max(y) })
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
            (Value::Float(f), "abs") => Value::Float(f.abs()),
            (Value::Float(f), "sqrt") => {
                if *f < 0.0 {
                    return self.fail("R014", "square root of a negative number", span);
                }
                Value::Float(f.sqrt())
            }
            (Value::Float(f), "floor" | "ceil" | "round") => {
                let r = match name {
                    "floor" => f.floor(),
                    "ceil" => f.ceil(),
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
        if f > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else {
        format!("{f:?}")
    }
}
