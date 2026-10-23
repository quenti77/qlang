//! Statements and expressions.

use super::unify::LangImpl;
use super::*;
use crate::ast::*;

/// Variables narrowed to non-`none` by a condition: `(name, type)` pairs.
type Facts = Vec<(String, Ty)>;

/// What the context needs from an expression.
#[derive(Clone)]
pub(crate) enum Want {
    /// The value is thrown away.
    Unused,
    /// The value is used, with no particular type expected.
    Any,
    /// The value is used and should have this type.
    Exp(Ty),
}

impl Want {
    pub fn used(&self) -> bool {
        !matches!(self, Want::Unused)
    }
    pub fn expected(&self) -> Option<&Ty> {
        match self {
            Want::Exp(t) => Some(t),
            _ => None,
        }
    }
}

pub(crate) enum VarOrigin {
    Local,
    Module(ModId),
}

impl Checker {
    // ----------------------------------------------------------------- scopes

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// For editors: `Type` or `module` written alone (as in `Type.|`) still has members.
    fn record_head(&mut self, e: &Expr) {
        if self.collecting() {
            let before = self.diags.len();
            let _ = self.try_head(e);
            self.diags.truncate(before);
        }
    }

    pub fn declare(&mut self, name: &Ident, local: Local) {
        let m = self.cur_mod;
        let target = match self.scopes.last_mut() {
            Some(s) => s,
            None => &mut self.mscopes[m].vars,
        };
        if let Some(prev) = target.get(&name.name)
            && prev.orig.is_none()
            && name.name != "_"
        {
            crate::diag::push_capped(
                &mut self.diags,
                Diagnostic::error(
                    "T150",
                    format!("`{}` is already declared in this scope", name.name),
                    name.span,
                ),
            );
            return;
        }
        target.insert(name.name.clone(), local.clone());
        if self.collect.is_some() {
            // a variable can be used from here to the end of its function (or file)
            let scope = match self.cur_fn.and_then(|f| self.defs.fns[f].decl.clone()) {
                Some(d) => d.span,
                None => Span {
                    file: name.span.file,
                    start: 0,
                    end: u32::MAX,
                },
            };
            if let Some(c) = &mut self.collect {
                c.local(name, &local, scope);
            }
        }
    }

    /// A variable visible here: locals first, then top-level variables.
    pub fn lookup_var(&self, name: &str) -> Option<(Local, VarOrigin)> {
        for s in self.scopes.iter().rev() {
            if let Some(l) = s.get(name) {
                return Some((l.clone(), VarOrigin::Local));
            }
        }
        if let Some(l) = self.mscopes[self.cur_mod].vars.get(name) {
            return Some((l.clone(), VarOrigin::Module(self.cur_mod)));
        }
        None
    }

    /// Show `name` with a narrower type from here on (until the scope ends).
    pub fn narrow(&mut self, name: &str, ty: Ty) {
        // a function that assigns `name` could set it back to `none` after the test
        if self.mscopes[self.cur_mod]
            .assigned_in_closures
            .contains(name)
        {
            return;
        }
        let Some((l, _)) = self.lookup_var(name) else {
            return;
        };
        let orig = l.orig.clone().unwrap_or_else(|| l.ty.clone());
        let entry = Local {
            ty,
            orig: Some(orig),
            ..l
        };
        let m = self.cur_mod;
        match self.scopes.last_mut() {
            Some(s) => {
                s.insert(name.to_string(), entry);
            }
            None => {
                self.mscopes[m].vars.insert(name.to_string(), entry);
            }
        }
    }

    /// Facts learned from a condition: variables that are not `none` when it
    /// is true, and when it is false.
    fn cond_facts(&mut self, c: &Expr) -> (Facts, Facts) {
        match &c.kind {
            ExprKind::Binary(op @ (BinOp::Eq | BinOp::NotEq), l, r) => {
                let name = match (&l.kind, &r.kind) {
                    (ExprKind::Ident(n, _), ExprKind::None)
                    | (ExprKind::None, ExprKind::Ident(n, _)) => n.clone(),
                    _ => return (vec![], vec![]),
                };
                let Some((local, _)) = self.lookup_var(&name) else {
                    return (vec![], vec![]);
                };
                let Ty::Nullable(inner) = self.resolve(&local.ty) else {
                    return (vec![], vec![]);
                };
                let fact = vec![(name, *inner)];
                if *op == BinOp::NotEq {
                    (fact, vec![])
                } else {
                    (vec![], fact)
                }
            }
            ExprKind::Binary(BinOp::And, a, b) => {
                let (mut ta, _) = self.cond_facts(a);
                let (tb, _) = self.cond_facts(b);
                ta.extend(tb);
                (ta, vec![])
            }
            ExprKind::Binary(BinOp::Or, a, b) => {
                let (_, mut fa) = self.cond_facts(a);
                let (_, fb) = self.cond_facts(b);
                fa.extend(fb);
                (vec![], fa)
            }
            ExprKind::Unary(UnOp::Not, a) => {
                let (t, f) = self.cond_facts(a);
                (f, t)
            }
            _ => (vec![], vec![]),
        }
    }

    // ------------------------------------------------------------ diagnostics

    pub fn mismatch(&mut self, span: Span, expected: &Ty, found: &Ty) {
        let (e, f) = (self.show(expected), self.show(found));
        let mut hint = String::new();
        let (re, rf) = (self.resolve(expected), self.resolve(found));
        if let Ty::Nullable(inner) = &rf {
            if **inner == re || !matches!(re, Ty::Nullable(_)) && self.assignable(inner, &re) {
                hint = " (this value may be `none`: check `!= none` first)".to_string();
            }
        } else if matches!((&re, &rf), (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int)) {
            hint = format!(" (convert explicitly with `as {e}`)");
        } else if rf == Ty::Unit {
            hint = " (this expression has no value)".to_string();
        }
        self.err("T002", format!("expected `{e}`, found `{f}`{hint}"), span);
    }

    /// Report a value-less expression used as a value.
    pub fn require_value(&mut self, t: Ty, span: Span) -> Ty {
        if t == Ty::Unit {
            self.err("T003", "this expression has no value", span);
            Ty::Error
        } else {
            t
        }
    }

    pub fn check_vis(&mut self, vis: Visibility, owner: StructId, span: Span, what: &str) {
        let ok = match vis {
            Visibility::Public => true,
            Visibility::Private => self.cur_impl_struct == Some(owner),
            Visibility::Protected => self
                .cur_impl_struct
                .is_some_and(|s| self.defs.is_subclass(s, owner)),
        };
        if !ok {
            let owner_name = self.defs.structs[owner].name.clone();
            let kind = if vis == Visibility::Private {
                "private"
            } else {
                "protected"
            };
            self.err("T160", format!("{what} is {kind} in `{owner_name}`"), span);
        }
    }

    // ------------------------------------------------------------- statements

    pub fn check_block(&mut self, b: &Block, want: &Want) -> Ty {
        self.push_scope();
        let t = self.check_stmts(&b.stmts, want);
        self.pop_scope();
        t
    }

    pub fn check_stmts(&mut self, stmts: &[Stmt], want: &Want) -> Ty {
        let mut ty = Ty::Unit;
        let mut diverges = false;
        for (i, s) in stmts.iter().enumerate() {
            let last = i + 1 == stmts.len();
            let w = if last { want.clone() } else { Want::Unused };
            let t = self.check_stmt(s, &w);
            if t == Ty::Never {
                diverges = true;
            }
            if last {
                ty = t;
            }
        }
        if diverges { Ty::Never } else { ty }
    }

    pub fn check_stmt(&mut self, s: &Stmt, want: &Want) -> Ty {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e, want),
            StmtKind::Let {
                is_const,
                name,
                ty,
                init,
                ..
            } => {
                let ann = ty.as_ref().map(|t| self.resolve_type(t));
                let w = match &ann {
                    Some(t) => Want::Exp(t.clone()),
                    None => Want::Any,
                };
                let it = self.expr(init, &w);
                let var_ty = match ann {
                    Some(a) => {
                        if !self.assignable(&it, &a) {
                            self.mismatch(init.span, &a, &it);
                        }
                        a
                    }
                    None => {
                        let it = self.resolve(&it);
                        match &it {
                            Ty::NoneT => {
                                self.err(
                                    "T170",
                                    format!("cannot infer the type of `{0}` from `none`: write `let {0}: type? = none`", name.name),
                                    init.span,
                                );
                                Ty::Error
                            }
                            Ty::Map(k, _) if **k == Ty::Never => {
                                self.err(
                                    "T170",
                                    format!("cannot infer the types of `{0}`: write `let {0}: map<key, value> = {{}}`", name.name),
                                    init.span,
                                );
                                Ty::Error
                            }
                            Ty::Array(inner) if **inner == Ty::Never => {
                                self.err(
                                    "T170",
                                    format!("cannot infer the element type of `{0}`: write `let {0}: array<type> = []`", name.name),
                                    init.span,
                                );
                                Ty::Error
                            }
                            Ty::Unit => {
                                self.err("T003", "this expression has no value", init.span);
                                Ty::Error
                            }
                            Ty::Never => Ty::Error,
                            _ => it,
                        }
                    }
                };
                let kind = if *is_const {
                    LocalKind::Const
                } else {
                    LocalKind::Var
                };
                self.declare(
                    name,
                    Local {
                        ty: var_ty,
                        kind,
                        orig: None,
                        span: name.span,
                    },
                );
                Ty::Unit
            }
            StmtKind::Seq { stmts, arity } => {
                for (i, st) in stmts.iter().enumerate() {
                    self.check_stmt(st, &Want::Unused);
                    // the first statement holds the value: it must be a tuple of the right size
                    if i == 0
                        && let (Some((n, span)), StmtKind::Let { name, .. }) = (arity, &st.kind)
                        && let Some((hidden, _)) = self.lookup_var(&name.name)
                    {
                        match self.resolve(&hidden.ty) {
                            Ty::Tuple(ts) if ts.len() != *n => {
                                self.err(
                                    "T288",
                                    format!(
                                        "the assignment takes {n} values, but the tuple has {}",
                                        ts.len()
                                    ),
                                    *span,
                                );
                                break; // the elements would only add noise
                            }
                            Ty::Tuple(_) | Ty::Error => {}
                            other => {
                                let shown = self.show(&other);
                                self.err(
                                    "T289",
                                    format!("`{shown}` cannot be taken apart: only a tuple can"),
                                    *span,
                                );
                                break;
                            }
                        }
                    }
                }
                Ty::Unit
            }
            StmtKind::LetTuple {
                is_const,
                pattern,
                ty,
                init,
            } => {
                let ann = ty.as_ref().map(|t| self.resolve_type(t));
                let w = match &ann {
                    Some(t) => Want::Exp(t.clone()),
                    None => Want::Any,
                };
                let it = self.expr(init, &w);
                let value_ty = match ann {
                    Some(a) => {
                        if !self.assignable(&it, &a) {
                            self.mismatch(init.span, &a, &it);
                        }
                        a
                    }
                    None => {
                        let it = self.resolve(&it);
                        match it {
                            Ty::Unit => {
                                self.err("T003", "this expression has no value", init.span);
                                Ty::Error
                            }
                            Ty::Never => Ty::Error,
                            other => other,
                        }
                    }
                };
                let kind = if *is_const {
                    LocalKind::Const
                } else {
                    LocalKind::Var
                };
                self.bind_tuple_pattern(pattern, &value_ty, kind);
                Ty::Unit
            }
            StmtKind::Return(value) => {
                let ret = self.ret_stack.last().cloned().unwrap_or(Ty::Unit);
                match value {
                    Some(e) => {
                        if ret == Ty::Unit {
                            let t = self.expr(e, &Want::Unused);
                            let _ = t;
                            self.err("T180", "this function does not return a value (add `-> type` to its signature)", e.span);
                        } else {
                            let t = self.expr(e, &Want::Exp(ret.clone()));
                            if !self.assignable(&t, &ret) {
                                self.mismatch(e.span, &ret, &t);
                            }
                        }
                    }
                    None => {
                        if ret != Ty::Unit {
                            let r = self.show(&ret);
                            self.err(
                                "T180",
                                format!("`return` needs a value of type `{r}`"),
                                s.span,
                            );
                        }
                    }
                }
                Ty::Never
            }
            StmtKind::While { cond, body } => {
                self.check_cond(cond);
                let (facts, _) = self.cond_facts(cond);
                self.loop_depth += 1;
                self.push_scope();
                for (n, t) in facts {
                    self.narrow(&n, t);
                }
                self.check_block(body, &Want::Unused);
                self.pop_scope();
                self.loop_depth -= 1;
                Ty::Unit
            }
            StmtKind::For {
                var,
                var2,
                var_pattern,
                var2_pattern,
                iter,
                step,
                body,
            } => {
                let it = self.expr(iter, &Want::Any);
                let it = self.resolve(&it);
                // the type of the (first, second) loop variables
                let (elem, elem2) = match (&it, var2.is_some()) {
                    (Ty::Range, false) => (Ty::Int, Ty::Error),
                    (Ty::Array(t), false) => ((**t).clone(), Ty::Error),
                    (Ty::Str, false) => (Ty::Str, Ty::Error),
                    (Ty::Array(t), true) => (Ty::Int, (**t).clone()),
                    (Ty::Str, true) => (Ty::Int, Ty::Str),
                    (Ty::Map(k, v), true) => ((**k).clone(), (**v).clone()),
                    // a `Set<T>` of the prelude gives its values
                    (Ty::Struct(sid, args), false)
                        if self.is_prelude_set(*sid) && args.len() == 1 =>
                    {
                        (args[0].clone(), Ty::Error)
                    }
                    (Ty::Error, _) => (Ty::Error, Ty::Error),
                    (other, two) => {
                        let shown = other.show(&self.defs);
                        let msg = if two {
                            format!(
                                "cannot loop over `{shown}` with two variables (use a map, an array or a string)"
                            )
                        } else if matches!(other, Ty::Map(..)) {
                            format!(
                                "a `{shown}` has keys and values: write `for key, value in m do` or `for key in m.keys() do`"
                            )
                        } else {
                            format!(
                                "cannot loop over `{shown}` (use a range like `0..10`, an array, a string or a map)"
                            )
                        };
                        self.err("T190", msg, iter.span);
                        (Ty::Error, Ty::Error)
                    }
                };
                if let Some(st) = step {
                    let stt = self.expr(st, &Want::Exp(Ty::Int));
                    if !self.assignable(&stt, &Ty::Int) {
                        self.mismatch(st.span, &Ty::Int, &stt);
                    }
                    if it != Ty::Range && !it.is_error() {
                        self.err("T190", "`step` can only be used with a range", st.span);
                    }
                }
                self.loop_depth += 1;
                self.push_scope();
                match var_pattern {
                    Some(p) => self.bind_tuple_pattern(p, &elem, LocalKind::Var),
                    None => self.declare(
                        var,
                        Local {
                            ty: elem,
                            kind: LocalKind::Var,
                            orig: None,
                            span: var.span,
                        },
                    ),
                }
                if let Some(v2) = var2 {
                    match var2_pattern {
                        Some(p) => self.bind_tuple_pattern(p, &elem2, LocalKind::Var),
                        None => self.declare(
                            v2,
                            Local {
                                ty: elem2,
                                kind: LocalKind::Var,
                                orig: None,
                                span: v2.span,
                            },
                        ),
                    }
                }
                self.check_block(body, &Want::Unused);
                self.pop_scope();
                self.loop_depth -= 1;
                Ty::Unit
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loop_depth == 0 {
                    let w = if matches!(s.kind, StmtKind::Break) {
                        "break"
                    } else {
                        "continue"
                    };
                    self.err(
                        "T191",
                        format!("`{w}` is only allowed inside a loop"),
                        s.span,
                    );
                }
                Ty::Never
            }
        }
    }

    /// Declares the variables of a tuple pattern with the types of the matching elements of
    /// `ty` (`let (a, (b, _)) = t`, `for (k, v) in pairs`).
    fn bind_tuple_pattern(&mut self, pat: &TuplePattern, ty: &Ty, kind: LocalKind) {
        match pat {
            TuplePattern::Wild(_) => {}
            TuplePattern::Name(n) => {
                let ty = self.resolve(ty);
                self.declare(
                    n,
                    Local {
                        ty,
                        kind,
                        orig: None,
                        span: n.span,
                    },
                );
            }
            TuplePattern::Tuple(items, span) => match self.resolve(ty) {
                Ty::Tuple(ts) if ts.len() == items.len() => {
                    for (p, t) in items.iter().zip(ts.iter()) {
                        self.bind_tuple_pattern(p, t, kind);
                    }
                }
                found => {
                    match &found {
                        Ty::Tuple(ts) => {
                            let msg = format!(
                                "the pattern takes {} values, but the tuple has {}",
                                items.len(),
                                ts.len()
                            );
                            self.err("T288", msg, *span);
                        }
                        Ty::Error => {}
                        other => {
                            let shown = self.show(other);
                            self.err(
                                "T289",
                                format!("`{shown}` cannot be taken apart: only a tuple can"),
                                *span,
                            );
                        }
                    }
                    // still declare the names, so they are not reported as unknown
                    for p in items {
                        self.bind_tuple_pattern(p, &Ty::Error, kind);
                    }
                }
            },
        }
    }

    /// The prelude's `Set` struct.
    pub(crate) fn is_prelude_set(&self, sid: StructId) -> bool {
        let s = &self.defs.structs[sid];
        s.module == PRELUDE_MOD && s.name == "Set"
    }

    fn check_cond(&mut self, c: &Expr) {
        let t = self.expr(c, &Want::Exp(Ty::Bool));
        if !self.assignable(&t, &Ty::Bool) {
            let shown = self.show(&t);
            self.err(
                "T004",
                format!("a condition must be a `bool`, found `{shown}` (write an explicit comparison such as `!= none` or `> 0`)"),
                c.span,
            );
        }
    }

    // ------------------------------------------------------------ expressions

    pub fn expr(&mut self, e: &Expr, want: &Want) -> Ty {
        let t = self.expr_inner(e, want);
        let t = self.resolve(&t);
        if let Some(c) = &mut self.collect {
            c.expr(e, t.clone());
        }
        t
    }

    fn expr_inner(&mut self, e: &Expr, want: &Want) -> Ty {
        match &e.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::None => Ty::NoneT,
            ExprKind::Str(parts) => {
                for p in parts {
                    if let StrPart::Expr(inner) = p {
                        let t = self.expr(inner, &Want::Any);
                        self.require_value(t, inner.span);
                    }
                }
                Ty::Str
            }
            ExprKind::Ident(name, targs) => self.check_ident(e, name, targs),
            ExprKind::SelfVal => match self.lookup_var("self") {
                Some((l, _)) => l.ty,
                None => {
                    self.err(
                        "T151",
                        "`self` is only available inside a method that takes `self`",
                        e.span,
                    );
                    Ty::Error
                }
            },
            ExprKind::Super => {
                self.err(
                    "T152",
                    "`super` can only be used as `super.method(...)`",
                    e.span,
                );
                Ty::Error
            }
            ExprKind::Array(items) => {
                let t = self.check_array(items, want);
                let r = self.resolve(&t);
                if matches!(&r, Ty::Array(el) if matches!(**el, Ty::Int | Ty::Float | Ty::Bool)) {
                    self.res.array_lits.insert(e.id, r);
                }
                t
            }
            ExprKind::Map(items) => self.check_map(items, want),
            ExprKind::StructLit(lit) => self.check_struct_lit(e, lit, want),
            ExprKind::Unary(op, inner) => {
                let t = self.expr(inner, &Want::Any);
                match op {
                    UnOp::Not => {
                        if !self.assignable(&t, &Ty::Bool) {
                            let shown = self.show(&t);
                            self.err(
                                "T004",
                                format!("`not` needs a `bool`, found `{shown}`"),
                                inner.span,
                            );
                        }
                        Ty::Bool
                    }
                    UnOp::Neg => match self.find_lang_impl(&t, Lang::Neg, &[]) {
                        Some(i) => i.ret(),
                        None => {
                            let shown = self.show(&t);
                            self.err("T005", format!("cannot negate a `{shown}`"), e.span);
                            Ty::Error
                        }
                    },
                }
            }
            ExprKind::Binary(op, l, r) => self.check_binary(e, *op, l, r),
            ExprKind::Assign { op, target, value } => self.check_assign(e, *op, target, value),
            ExprKind::Range { start, end, .. } => {
                for x in [start, end] {
                    let t = self.expr(x, &Want::Exp(Ty::Int));
                    if !self.assignable(&t, &Ty::Int) {
                        self.mismatch(x.span, &Ty::Int, &t);
                    }
                }
                Ty::Range
            }
            ExprKind::Try(inner) => self.check_try(e, inner),
            ExprKind::Tuple(items) => {
                let expected: Option<Vec<Ty>> = match want.expected().map(|t| self.resolve(t)) {
                    Some(Ty::Tuple(ts)) if ts.len() == items.len() => Some(ts),
                    _ => None,
                };
                let mut types = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    let w = match &expected {
                        Some(ts) => Want::Exp(ts[i].clone()),
                        None => Want::Any,
                    };
                    let t = self.expr(item, &w);
                    types.push(self.require_value(t, item.span));
                }
                Ty::Tuple(types)
            }
            ExprKind::TupleField(inner, index) => {
                let t = self.expr(inner, &Want::Any);
                let t = self.require_value(t, inner.span);
                match self.resolve(&t) {
                    Ty::Tuple(ts) => match ts.get(*index) {
                        Some(el) => el.clone(),
                        None => {
                            let shown = self.show(&Ty::Tuple(ts.clone()));
                            self.err(
                                "T286",
                                format!(
                                    "`{shown}` has {} elements, so `.{index}` does not exist (they are numbered from 0)",
                                    ts.len()
                                ),
                                e.span,
                            );
                            Ty::Error
                        }
                    },
                    Ty::Error => Ty::Error,
                    Ty::Nullable(_) | Ty::NoneT => {
                        let shown = self.show(&t);
                        self.err(
                            "T162",
                            format!("this value may be `none` (`{shown}`): check `!= none` before using `.{index}`"),
                            e.span,
                        );
                        Ty::Error
                    }
                    other => {
                        let shown = self.show(&other);
                        self.err(
                            "T287",
                            format!("`.{index}` needs a tuple, found `{shown}`"),
                            e.span,
                        );
                        Ty::Error
                    }
                }
            }
            ExprKind::Cast(inner, te) => {
                let from = self.expr(inner, &Want::Any);
                let from = self.require_value(from, inner.span);
                let to = self.resolve_type(te);
                if !self.cast_ok(&from, &to) {
                    let (a, b) = (self.show(&from), self.show(&to));
                    let hint = if matches!(self.resolve(&from), Ty::Nullable(_)) {
                        " (this value may be `none`)"
                    } else {
                        ""
                    };
                    self.err(
                        "T006",
                        format!("cannot convert `{a}` to `{b}` with `as` (a conversion needs an `impl As<{b}> for {a}`){hint}"),
                        e.span,
                    );
                }
                self.res.casts.insert(e.id, to.clone());
                to
            }
            ExprKind::Call { callee, args } => self.check_call(e, callee, args, want),
            ExprKind::Index(base, idx) => {
                let bt = self.expr(base, &Want::Any);
                let it = self.expr(idx, &Want::Any);
                match self.find_lang_impl(&bt, Lang::Index, std::slice::from_ref(&it)) {
                    Some(i) => i.ret(),
                    None => {
                        let (b, i) = (self.show(&bt), self.show(&it));
                        let hint = if matches!(self.resolve(&bt), Ty::Nullable(_)) {
                            " (this value may be `none`)"
                        } else {
                            ""
                        };
                        self.err(
                            "T007",
                            format!("cannot index a `{b}` with a `{i}`{hint}"),
                            e.span,
                        );
                        Ty::Error
                    }
                }
            }
            ExprKind::Append(_) => {
                self.err(
                    "T008",
                    "`x[]` can only be used as the target of an assignment: `x[] = value`",
                    e.span,
                );
                Ty::Error
            }
            ExprKind::Field(obj, name, targs) => self.check_field(e, obj, name, targs),
            ExprKind::If {
                branches,
                else_block,
            } => self.check_if(e, branches, else_block.as_ref(), want),
            ExprKind::Match {
                scrutinee,
                cases,
                else_block,
            } => self.check_match(scrutinee, cases, else_block.as_ref(), want),
            ExprKind::Lambda(f) => self.check_lambda(f),
        }
    }

    /// Remembers the type of a use of a variable when it is `int`, `float` or `bool`.
    fn note_ident_ty(&mut self, id: NodeId, ty: &Ty) {
        let t = self.resolve(ty);
        let scalar = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Bool);
        if scalar(&t) || t == Ty::Str || matches!(&t, Ty::Array(e) if scalar(&self.resolve(e))) {
            self.res.ident_tys.insert(id, t);
        }
    }

    fn check_ident(&mut self, e: &Expr, name: &str, targs: &[TypeExpr]) -> Ty {
        if let Some((local, origin)) = self.lookup_var(name) {
            let res = match origin {
                VarOrigin::Local => PathRes::Local,
                VarOrigin::Module(m) => PathRes::Global(m, name.to_string()),
            };
            self.res.paths.insert(e.id, res);
            self.note_ident_ty(e.id, &local.ty);
            return local.ty;
        }
        match self.lookup_item(name) {
            Some(ItemRef::Fn(fid)) => self.fn_value(e, fid, targs),
            Some(ItemRef::Var(m, vname)) => match self.mscopes[m].vars.get(&vname).cloned() {
                Some(l) => {
                    self.res.paths.insert(e.id, PathRes::Global(m, vname));
                    self.note_ident_ty(e.id, &l.ty);
                    l.ty
                }
                None => {
                    self.err(
                        "T153",
                        format!("`{name}` is used before it is declared"),
                        e.span,
                    );
                    Ty::Error
                }
            },
            Some(ItemRef::Struct(_)) => {
                self.record_head(e);
                self.err(
                    "T154",
                    format!("`{name}` is a type, not a value (build one with `{name} {{ ... }}` or call its methods like `{name}.new(...)`)"),
                    e.span,
                );
                Ty::Error
            }
            Some(ItemRef::Enum(_)) | Some(ItemRef::Trait(_)) => {
                self.record_head(e);
                self.err("T154", format!("`{name}` is a type, not a value"), e.span);
                Ty::Error
            }
            Some(ItemRef::Module(_)) => {
                self.record_head(e);
                self.err(
                    "T154",
                    format!("`{name}` is a module: use `{name}.something`"),
                    e.span,
                );
                Ty::Error
            }
            None => {
                self.record_head(e);
                if decls::BUILTIN_FNS.contains(&name) {
                    self.err(
                        "T155",
                        format!("`{name}` is a built-in function: call it as `{name}(...)`"),
                        e.span,
                    );
                } else {
                    self.err("T001", format!("unknown name `{name}`"), e.span);
                }
                Ty::Error
            }
        }
    }

    /// A function used as a value (not called).
    pub fn fn_value(&mut self, e: &Expr, fid: FnId, targs: &[TypeExpr]) -> Ty {
        let def = self.defs.fns[fid].clone();
        self.check_extern_available(fid, e.span);
        self.res.paths.insert(e.id, PathRes::Fn(fid));
        let own: Vec<ParamId> = def
            .scope_params
            .iter()
            .rev()
            .take(def.own_tparams)
            .rev()
            .map(|(_, p)| *p)
            .collect();
        if own.is_empty() {
            if !targs.is_empty() {
                self.err("T121", format!("`{}` is not generic", def.name), e.span);
            }
            return Ty::Fn(def.params.clone(), Box::new(def.ret.clone()));
        }
        if targs.len() != own.len() {
            self.err(
                "T121",
                format!(
                    "`{}` is generic: it must be called, or given its type arguments (`{}<...>`)",
                    def.name, def.name
                ),
                e.span,
            );
            return Ty::Error;
        }
        let map: HashMap<ParamId, Ty> = own
            .iter()
            .cloned()
            .zip(targs.iter().map(|t| self.resolve_type(t)))
            .collect();
        Ty::Fn(
            def.params.iter().map(|p| p.subst(&map)).collect(),
            Box::new(def.ret.subst(&map)),
        )
    }

    fn check_map(&mut self, items: &[(Expr, Expr)], want: &Want) -> Ty {
        let hint = match want.expected().map(|t| self.resolve(t)) {
            Some(Ty::Map(k, v)) => Some((*k, *v)),
            Some(Ty::Nullable(inner)) => match *inner {
                Ty::Map(k, v) => Some((*k, *v)),
                _ => None,
            },
            _ => None,
        };
        if items.is_empty() {
            return match hint {
                Some((k, v)) => Ty::Map(Box::new(k), Box::new(v)),
                None => Ty::Map(Box::new(Ty::Never), Box::new(Ty::Never)),
            };
        }
        let (mut kt, mut vt): (Option<Ty>, Option<Ty>) = (None, None);
        for (k, v) in items {
            for (is_key, x) in [(true, k), (false, v)] {
                let slot = if is_key { &kt } else { &vt };
                let h = hint
                    .as_ref()
                    .map(|(hk, hv)| if is_key { hk.clone() } else { hv.clone() });
                let w = match (slot, &h) {
                    (Some(t), _) => Want::Exp(t.clone()),
                    (None, Some(h)) => Want::Exp(h.clone()),
                    _ => Want::Any,
                };
                let t = self.expr(x, &w);
                let t = self.require_value(t, x.span);
                let merged = match slot.clone() {
                    None => match &h {
                        Some(h) if self.assignable(&t, h) => h.clone(),
                        _ => t,
                    },
                    Some(prev) => match self.join(&prev, &t) {
                        Some(j) => j,
                        None => {
                            let (a, b) = (self.show(&prev), self.show(&t));
                            let what = if is_key { "keys" } else { "values" };
                            self.err(
                                "T009",
                                format!(
                                    "map {what} must have the same type: found `{a}` and `{b}`"
                                ),
                                x.span,
                            );
                            prev
                        }
                    },
                };
                if is_key {
                    kt = Some(merged);
                } else {
                    vt = Some(merged);
                }
            }
        }
        let (kt, vt) = (kt.unwrap(), vt.unwrap());
        self.check_key_type(&kt, items[0].0.span);
        Ty::Map(Box::new(kt), Box::new(vt))
    }

    fn check_array(&mut self, items: &[Expr], want: &Want) -> Ty {
        let hint = match want.expected().map(|t| self.resolve(t)) {
            Some(Ty::Array(t)) => Some(*t),
            Some(Ty::Nullable(inner)) => match *inner {
                Ty::Array(t) => Some(*t),
                _ => None,
            },
            _ => None,
        };
        if items.is_empty() {
            return Ty::Array(Box::new(hint.unwrap_or(Ty::Never)));
        }
        let mut elem: Option<Ty> = None;
        for it in items {
            let w = match (&elem, &hint) {
                (Some(t), _) => Want::Exp(t.clone()),
                (None, Some(h)) => Want::Exp(h.clone()),
                _ => Want::Any,
            };
            let t = self.expr(it, &w);
            let t = self.require_value(t, it.span);
            elem = Some(match elem {
                None => match &hint {
                    Some(h) if self.assignable(&t, h) => h.clone(),
                    _ => t,
                },
                Some(prev) => match self.join(&prev, &t) {
                    Some(j) => j,
                    None => {
                        let (a, b) = (self.show(&prev), self.show(&t));
                        self.err(
                            "T009",
                            format!(
                                "array elements must have the same type: found `{a}` and `{b}`"
                            ),
                            it.span,
                        );
                        prev
                    }
                },
            });
        }
        Ty::Array(Box::new(elem.unwrap()))
    }

    fn check_binary(&mut self, e: &Expr, op: BinOp, l: &Expr, r: &Expr) -> Ty {
        match op {
            BinOp::And | BinOp::Or => {
                for (i, x) in [l, r].into_iter().enumerate() {
                    // `a != none and a > 2`: the right side sees what the left side proved
                    self.push_scope();
                    if i == 1 {
                        let (when_true, when_false) = self.cond_facts(l);
                        for (n, t) in if op == BinOp::And {
                            when_true
                        } else {
                            when_false
                        } {
                            self.narrow(&n, t);
                        }
                    }
                    let t = self.expr(x, &Want::Exp(Ty::Bool));
                    self.pop_scope();
                    if !self.assignable(&t, &Ty::Bool) {
                        let shown = self.show(&t);
                        self.err(
                            "T004",
                            format!("`{}` needs `bool` operands, found `{shown}`", op.symbol()),
                            x.span,
                        );
                    }
                }
                Ty::Bool
            }
            _ => {
                let lt = self.expr(l, &Want::Any);
                // the right operand benefits from the left type (for `none`, literals...)
                let rt = self.expr(r, &Want::Any);
                self.binop_type(op, &lt, &rt, e.span)
            }
        }
    }

    pub fn binop_type(&mut self, op: BinOp, lt: &Ty, rt: &Ty, span: Span) -> Ty {
        let (lt, rt) = (self.resolve(lt), self.resolve(rt));
        if lt.is_error() || rt.is_error() {
            return Ty::Error;
        }
        let ok = match op {
            BinOp::Eq | BinOp::NotEq => {
                if self.eq_ok(&lt, &rt) {
                    Some(Ty::Bool)
                } else {
                    let (a, b) = (self.show(&lt), self.show(&rt));
                    self.err("T005", format!("cannot compare `{a}` with `{b}`"), span);
                    return Ty::Bool;
                }
            }
            BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => self
                .find_lang_impl(&lt, Lang::Ord, std::slice::from_ref(&rt))
                .map(|_| Ty::Bool),
            _ => {
                let lang = match op {
                    BinOp::Add => Lang::Add,
                    BinOp::Sub => Lang::Sub,
                    BinOp::Mul => Lang::Mul,
                    BinOp::Div => Lang::Div,
                    BinOp::IntDiv => Lang::IntDiv,
                    BinOp::Mod => Lang::Mod,
                    _ => Lang::Pow,
                };
                self.find_lang_impl(&lt, lang, std::slice::from_ref(&rt))
                    .map(|i: LangImpl| i.ret())
            }
        };
        match ok {
            Some(t) => t,
            None => {
                let (a, b) = (self.show(&lt), self.show(&rt));
                let hint = if matches!(lt, Ty::Nullable(_)) || matches!(rt, Ty::Nullable(_)) {
                    " (a value may be `none`: check `!= none` first)"
                } else if matches!(op, BinOp::IntDiv) {
                    " (`div` is for integers; use `/` for decimals)"
                } else {
                    ""
                };
                self.err(
                    "T005",
                    format!("cannot apply `{}` to `{a}` and `{b}`{hint}", op.symbol()),
                    span,
                );
                Ty::Error
            }
        }
    }

    fn check_assign(&mut self, e: &Expr, op: Option<BinOp>, target: &Expr, value: &Expr) -> Ty {
        // a plain variable being assigned: its narrowed view is updated afterwards
        let mut assigned_var: Option<(String, Ty)> = None;
        // the type the target holds
        let tt: Ty = match &target.kind {
            ExprKind::Ident(name, _) => {
                match self.lookup_var(name) {
                    Some((l, origin)) => {
                        self.res.paths.insert(
                            target.id,
                            match origin {
                                VarOrigin::Local => PathRes::Local,
                                VarOrigin::Module(m) => PathRes::Global(m, name.clone()),
                            },
                        );
                        if l.kind == LocalKind::Const {
                            self.err_note(
                            "T210",
                            format!("cannot assign to `{name}`: it is a constant"),
                            target.span,
                            "constants (`const`) cannot be reassigned; use `let` for a variable",
                            Some(l.span),
                        );
                        }
                        let declared = l.orig.clone().unwrap_or_else(|| l.ty.clone());
                        self.note_ident_ty(target.id, &declared);
                        assigned_var = Some((name.clone(), declared.clone()));
                        declared
                    }
                    None => {
                        if let Some(ItemRef::Var(m, vname)) = self.lookup_item(name) {
                            if let Some(l) = self.mscopes[m].vars.get(&vname).cloned() {
                                if m != self.cur_mod {
                                    self.err("T211", format!("cannot assign to `{name}`: it belongs to another module"), target.span);
                                }
                                self.res.paths.insert(target.id, PathRes::Global(m, vname));
                                self.note_ident_ty(target.id, &l.ty);
                                l.ty
                            } else {
                                Ty::Error
                            }
                        } else {
                            self.err(
                                "T212",
                                format!("cannot assign to `{name}`: it is not a variable"),
                                target.span,
                            );
                            Ty::Error
                        }
                    }
                }
            }
            ExprKind::Field(obj, name, targs) => self.check_field_target(target, obj, name, targs),
            ExprKind::Index(base, idx) => {
                let bt = self.expr(base, &Want::Any);
                let it = self.expr(idx, &Want::Any);
                // the value type is checked below
                match op {
                    None => {
                        let vt = self.expr(value, &Want::Any);
                        return self.finish_index_assign(e, &bt, &it, &vt, value.span);
                    }
                    Some(bop) => {
                        let Some(read) =
                            self.find_lang_impl(&bt, Lang::Index, std::slice::from_ref(&it))
                        else {
                            let (b, i) = (self.show(&bt), self.show(&it));
                            self.err(
                                "T007",
                                format!("cannot index a `{b}` with a `{i}`"),
                                target.span,
                            );
                            self.expr(value, &Want::Any);
                            return Ty::Error;
                        };
                        // `m[k] += 1` on a map reads the value itself (an error at run time if absent)
                        let elem = match self.resolve(&bt) {
                            Ty::Map(_, v) => *v,
                            _ => read.ret(),
                        };
                        let vt = self.expr(value, &Want::Any);
                        let res = self.binop_type(bop, &elem, &vt, e.span);
                        return self.finish_index_assign(e, &bt, &it, &res, value.span);
                    }
                }
            }
            ExprKind::Append(base) => {
                if op.is_some() {
                    self.err(
                        "T008",
                        "`x[]` only works with `=`: `x[] = value`",
                        target.span,
                    );
                }
                let bt = self.expr(base, &Want::Any);
                let hint = match self.resolve(&bt) {
                    Ty::Array(t) => Want::Exp(*t),
                    _ => Want::Any,
                };
                let vt = self.expr(value, &hint);
                if self
                    .find_lang_impl(&bt, Lang::Push, std::slice::from_ref(&vt))
                    .is_none()
                {
                    let (b, v) = (self.show(&bt), self.show(&vt));
                    self.err(
                        "T008",
                        format!("cannot append a `{v}` to a `{b}` with `[] =` (an `impl Push<{v}> for {b}` is needed)"),
                        e.span,
                    );
                }
                return vt;
            }
            ExprKind::TupleField(..) => {
                self.err(
                    "T213",
                    "a tuple cannot be changed: build a new one (`t = (t.0, new_value)`)",
                    target.span,
                );
                Ty::Error
            }
            _ => {
                self.err(
                    "T213",
                    "invalid assignment target (use a variable, `a.field` or `a[i]`)",
                    target.span,
                );
                Ty::Error
            }
        };
        let assigned = match op {
            None => {
                let vt = self.expr(value, &Want::Exp(tt.clone()));
                if !self.assignable(&vt, &tt) {
                    self.mismatch(value.span, &tt, &vt);
                }
                vt
            }
            Some(bop) => {
                let vt = self.expr(value, &Want::Any);
                let res = self.binop_type(bop, &tt, &vt, e.span);
                if !self.assignable(&res, &tt) {
                    self.mismatch(e.span, &tt, &res);
                }
                res
            }
        };
        if let Some((name, declared)) = assigned_var {
            self.after_assign(&name, &declared, &assigned);
        }
        tt
    }

    /// After `x = value`, the old narrowing of `x` is gone; a value that is
    /// certainly not `none` narrows a nullable variable again.
    fn after_assign(&mut self, name: &str, declared: &Ty, value_ty: &Ty) {
        if !matches!(declared, Ty::Nullable(_)) {
            return;
        }
        let value_ty = self.resolve(value_ty);
        let now = if matches!(value_ty, Ty::NoneT | Ty::Nullable(_) | Ty::Error) {
            declared.clone()
        } else {
            declared.non_null()
        };
        self.narrow(name, now);
    }

    fn finish_index_assign(&mut self, e: &Expr, bt: &Ty, it: &Ty, vt: &Ty, vspan: Span) -> Ty {
        let args = [it.clone(), vt.clone()];
        if self.find_lang_impl(bt, Lang::IndexSet, &args).is_none() {
            let (b, i, v) = (self.show(bt), self.show(it), self.show(vt));
            let readable = self
                .find_lang_impl(bt, Lang::Index, std::slice::from_ref(it))
                .is_some();
            let msg = if readable {
                format!("cannot store a `{v}` at index `{i}` of a `{b}`")
            } else {
                format!("cannot set an element of a `{b}` with a `{i}` index")
            };
            let _ = vspan;
            self.err("T007", msg, e.span);
        }
        vt.clone()
    }

    // --------------------------------------------------------- conditionals

    fn check_if(
        &mut self,
        e: &Expr,
        branches: &[(Expr, Block)],
        else_block: Option<&Block>,
        want: &Want,
    ) -> Ty {
        let used = want.used();
        let mut result: Option<Ty> = None;
        let mut acc_else: Vec<(String, Ty)> = Vec::new();
        let mut all_never = true;
        let mut failed = false;
        let merge = |this: &mut Checker, result: &mut Option<Ty>, bt: Ty, failed: &mut bool| {
            if !used {
                return;
            }
            *result = Some(match result.take() {
                None => bt,
                Some(prev) => match this.join(&prev, &bt) {
                    Some(j) => j,
                    None => {
                        if !*failed {
                            let (a, b) = (this.show(&prev), this.show(&bt));
                            this.err(
                                "T009",
                                format!("the branches have different types: `{a}` and `{b}`"),
                                e.span,
                            );
                            *failed = true;
                        }
                        Ty::Error
                    }
                },
            });
        };
        for (cond, block) in branches {
            self.check_cond(cond);
            let (tfacts, ffacts) = self.cond_facts(cond);
            self.push_scope();
            for (n, t) in acc_else.clone() {
                self.narrow(&n, t);
            }
            for (n, t) in tfacts {
                self.narrow(&n, t);
            }
            let bt = self.check_block(block, want);
            self.pop_scope();
            if bt != Ty::Never {
                all_never = false;
            }
            merge(self, &mut result, bt, &mut failed);
            acc_else.extend(ffacts);
        }
        match else_block {
            Some(b) => {
                self.push_scope();
                for (n, t) in acc_else.clone() {
                    self.narrow(&n, t);
                }
                let bt = self.check_block(b, want);
                self.pop_scope();
                merge(self, &mut result, bt, &mut failed);
                if !used {
                    return Ty::Unit;
                }
                result.unwrap_or(Ty::Unit)
            }
            None => {
                // `if x == none then return end`: afterwards x is not none
                if all_never {
                    for (n, t) in acc_else {
                        self.narrow(&n, t);
                    }
                }
                Ty::Unit
            }
        }
    }

    fn check_match(
        &mut self,
        scrutinee: &Expr,
        cases: &[MatchCase],
        else_block: Option<&Block>,
        want: &Want,
    ) -> Ty {
        let used = want.used();
        let st = self.expr(scrutinee, &Want::Any);
        let st = self.require_value(st, scrutinee.span);
        let mut result: Option<Ty> = None;
        let mut failed = false;
        let mut none_seen = false;
        let mut covering: Vec<&Pattern> = Vec::new();
        for case in cases {
            self.push_scope();
            self.check_pattern(&case.pattern, case.pattern_span, &st, none_seen);
            if let Some(g) = &case.guard {
                self.check_cond(g);
            }
            if case.guard.is_none() {
                covering.push(&case.pattern);
                none_seen |= matches!(case.pattern, Pattern::None);
            }
            let bt = self.check_block(&case.body, want);
            self.pop_scope();
            if used {
                result = Some(match result.take() {
                    None => bt,
                    Some(prev) => match self.join(&prev, &bt) {
                        Some(j) => j,
                        None => {
                            if !failed {
                                let (a, b) = (self.show(&prev), self.show(&bt));
                                self.err(
                                    "T009",
                                    format!("the cases have different types: `{a}` and `{b}`"),
                                    case.span,
                                );
                                failed = true;
                            }
                            Ty::Error
                        }
                    },
                });
            }
        }
        if let Some(b) = else_block {
            let bt = self.check_block(b, want);
            if used {
                result = Some(match result.take() {
                    None => bt,
                    Some(prev) => match self.join(&prev, &bt) {
                        Some(j) => j,
                        None => {
                            if !failed {
                                let (a, b2) = (self.show(&prev), self.show(&bt));
                                self.err(
                                    "T009",
                                    format!("the cases have different types: `{a}` and `{b2}`"),
                                    b.span,
                                );
                            }
                            Ty::Error
                        }
                    },
                });
            }
        }
        let exhaustive = else_block.is_some() || self.patterns_exhaustive(&st, &covering);
        if !exhaustive || !used {
            return Ty::Unit;
        }
        result.unwrap_or(Ty::Unit)
    }

    fn check_pattern(&mut self, p: &Pattern, span: Span, st: &Ty, none_seen: bool) {
        let st_r = self.resolve(st);
        let inner = st_r.non_null();
        let expect = |this: &mut Checker, got: Ty| {
            if !this.assignable(&got, &inner) && !this.eq_ok(&got, &inner) {
                let (a, b) = (this.show(&inner), this.show(&got));
                this.err(
                    "T220",
                    format!("this pattern has type `{b}`, but the value matched has type `{a}`"),
                    span,
                );
            }
        };
        match p {
            Pattern::Tuple(items) => self.check_tuple_pattern(items, span, &inner),
            Pattern::Int(_) => expect(self, Ty::Int),
            Pattern::Float(_) => expect(self, Ty::Float),
            Pattern::Str(_) => expect(self, Ty::Str),
            Pattern::Bool(_) => expect(self, Ty::Bool),
            Pattern::None => {
                if !matches!(st_r, Ty::Nullable(_) | Ty::NoneT | Ty::Error) {
                    let shown = self.show(&st_r);
                    self.err("T220", format!("`none` can never match a `{shown}` (only nullable values, like `{shown}?`)"), span);
                }
            }
            Pattern::Range { lo, hi, .. } => {
                for b in [lo, hi] {
                    let t = match **b {
                        Pattern::Int(_) => Ty::Int,
                        _ => Ty::Float,
                    };
                    expect(self, t);
                }
            }
            Pattern::Path { path, id } => {
                if let Some((eid, vi, args)) = self.pattern_variant(path, &inner, span) {
                    let expected = Ty::Enum(eid, args);
                    self.res.patterns.insert(*id, (eid, vi));
                    if !self.defs.enums[eid].variants[vi].fields.is_empty() {
                        let v = &self.defs.enums[eid].variants[vi];
                        let msg = format!(
                            "`{}` carries data: write `{}({})` to take it",
                            v.name,
                            v.name,
                            v.fields.iter().map(|_| "_").collect::<Vec<_>>().join(", ")
                        );
                        self.err("T222", msg, span);
                    }
                    expect(self, expected);
                }
            }
            Pattern::Variant { path, fields, id } => {
                let Some((eid, vi, args)) = self.pattern_variant(path, &inner, span) else {
                    return;
                };
                self.res.patterns.insert(*id, (eid, vi));
                expect(self, Ty::Enum(eid, args.clone()));
                let def = self.defs.enums[eid].clone();
                let v = &def.variants[vi];
                if v.fields.len() != fields.len() {
                    self.err(
                        "T222",
                        format!(
                            "`{}` carries {} value(s), but the pattern names {}",
                            v.name,
                            v.fields.len(),
                            fields.len()
                        ),
                        span,
                    );
                    // one mistake, one error: still declare the names
                    for p in fields {
                        self.check_pattern(p, span, &Ty::Error, false);
                    }
                    return;
                }
                let map: HashMap<ParamId, Ty> = def
                    .tparams
                    .iter()
                    .copied()
                    .zip(args.iter().cloned())
                    .collect();
                for ((_, fty), p) in v.fields.iter().zip(fields) {
                    let ty = self.resolve(&fty.subst(&map));
                    self.check_pattern(p, span, &ty, false);
                }
            }
            Pattern::Bind { name, id } => {
                // a name that is a variant of the type being matched is that variant
                if let Ty::Enum(eid, args) = &inner
                    && let Some(vi) = self.defs.enums[*eid].variant_index(&name.name)
                {
                    self.res.patterns.insert(*id, (*eid, vi));
                    if !self.defs.enums[*eid].variants[vi].fields.is_empty() {
                        let msg =
                            format!("`{0}` carries data: write `{0}(...)` to take it", name.name);
                        self.err("T222", msg, span);
                    }
                    let _ = args;
                    return;
                }
                if name.name != "_" {
                    let ty = if none_seen {
                        inner.clone()
                    } else {
                        st_r.clone()
                    };
                    self.declare(
                        name,
                        Local {
                            ty,
                            kind: LocalKind::Var,
                            orig: None,
                            span: name.span,
                        },
                    );
                }
            }
        }
    }

    /// `(0, y)` against a value of type `ty`: one sub-pattern for each element.
    fn check_tuple_pattern(&mut self, items: &[Pattern], span: Span, ty: &Ty) {
        match self.resolve(ty) {
            Ty::Tuple(ts) if ts.len() == items.len() => {
                for (p, t) in items.iter().zip(ts.iter()) {
                    self.check_pattern(p, span, t, false);
                }
            }
            found => {
                match &found {
                    Ty::Tuple(ts) => {
                        let msg = format!(
                            "this pattern has {} elements, but the tuple matched has {}",
                            items.len(),
                            ts.len()
                        );
                        self.err("T220", msg, span);
                    }
                    Ty::Error => {}
                    other => {
                        let shown = self.show(other);
                        self.err(
                            "T220",
                            format!(
                                "this pattern is a tuple, but the value matched has type `{shown}`"
                            ),
                            span,
                        );
                    }
                }
                // declare the names anyway: one mistake, one error
                for p in items {
                    self.check_pattern(p, span, &Ty::Error, false);
                }
            }
        }
    }

    /// The variant a pattern path names: `Circle` (the type being matched is known),
    /// `Shape.Circle` or `m.Shape.Circle`; also the type arguments of its enum, those of
    /// the matched value when it is an instance of the same enum.
    fn pattern_variant(
        &mut self,
        path: &[Ident],
        matched: &Ty,
        span: Span,
    ) -> Option<(EnumId, usize, Vec<Ty>)> {
        let (eid, last) = if path.len() == 1 {
            let Ty::Enum(eid, _) = matched else {
                if !matches!(matched, Ty::Error) {
                    let shown = self.show(matched);
                    self.err(
                        "T221",
                        format!(
                            "`{}` is not a variant: the value matched has type `{shown}`",
                            path[0].name
                        ),
                        span,
                    );
                }
                return None;
            };
            (*eid, &path[0])
        } else {
            let (head, last) = path.split_at(path.len() - 1);
            let Some(ItemRef::Enum(eid)) = self.lookup_path_item(head, "enum") else {
                if !self.diags.iter().any(|d| d.span == head[0].span) {
                    self.err("T221", "expected `Enum.Variant`", span);
                }
                return None;
            };
            (eid, &last[0])
        };
        let Some(vi) = self.defs.enums[eid].variant_index(&last.name) else {
            let en = self.defs.enums[eid].name.clone();
            self.err(
                "T221",
                format!("enum `{en}` has no variant `{}`", last.name),
                last.span,
            );
            return None;
        };
        let args = match matched {
            Ty::Enum(m, args) if *m == eid => args.clone(),
            _ => (0..self.defs.enums[eid].tparams.len())
                .map(|_| self.fresh())
                .collect(),
        };
        Some((eid, vi, args))
    }

    /// `inner?`: the value of a `Result` or of a nullable, or leave the current function with
    /// the error (the same `Result` error type) or with `none`.
    fn check_try(&mut self, e: &Expr, inner: &Expr) -> Ty {
        let t = self.expr(inner, &Want::Any);
        let t = self.require_value(t, inner.span);
        let t = self.resolve(&t);
        let ret = self.ret_stack.last().cloned().map(|r| self.resolve(&r));
        if ret.is_none() {
            self.err(
                "T280",
                "`?` can only be used inside a function (it leaves the function when there is an error)",
                e.span,
            );
        }
        match &t {
            Ty::Error => Ty::Error,
            Ty::Enum(eid, args)
                if args.len() == 2
                    && self.defs.enums[*eid].module == PRELUDE_MOD
                    && self.defs.enums[*eid].name == "Result" =>
            {
                self.res.result_tries.insert(e.id);
                if let Some(ret) = ret {
                    let needed = Ty::Enum(*eid, vec![self.fresh(), args[1].clone()]);
                    if !matches!(ret, Ty::Error) && !self.assignable(&needed, &ret) {
                        let (shown, er) = (self.show(&ret), self.show(&args[1]));
                        self.err(
                            "T282",
                            format!("`?` on a `Result` leaves the function with its error, so the function must return a `Result<_, {er}>`, not `{shown}`"),
                            e.span,
                        );
                    }
                }
                args[0].clone()
            }
            Ty::Nullable(value) => {
                if let Some(ret) = ret
                    && !matches!(ret, Ty::Nullable(_) | Ty::Error)
                {
                    let shown = self.show(&ret);
                    self.err(
                        "T282",
                        format!("`?` on a nullable value leaves the function with `none`, so the function must return a nullable (`T?`), not `{shown}`"),
                        e.span,
                    );
                }
                (**value).clone()
            }
            other => {
                let shown = self.show(other);
                self.err(
                    "T281",
                    format!("`?` needs a `Result` or a nullable value, found `{shown}`"),
                    e.span,
                );
                Ty::Error
            }
        }
    }

    // ------------------------------------------------------------- lambdas

    fn check_lambda(&mut self, f: &Rc<FunDecl>) -> Ty {
        if f.has_self {
            self.err(
                "T127",
                "`self` is not a parameter of an anonymous function",
                f.span,
            );
        }
        let mark = self.tparam_scope.len();
        let own = self.make_tparams(&f.tparams);
        if !own.is_empty() {
            self.err("T128", "anonymous functions cannot be generic", f.span);
        }
        let params: Vec<Ty> = f.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
        let ret = f
            .ret
            .as_ref()
            .map(|r| self.resolve_type(r))
            .unwrap_or(Ty::Unit);
        self.tparam_scope.truncate(mark);
        let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
        self.ret_stack.push(ret.clone());
        self.push_scope();
        for (p, t) in f.params.iter().zip(params.iter()) {
            self.declare(
                &p.name,
                Local {
                    ty: t.clone(),
                    kind: LocalKind::Param,
                    orig: None,
                    span: p.name.span,
                },
            );
        }
        if let Some(body) = &f.body {
            self.check_body_result(body, &ret, f.span, "this function");
        }
        self.pop_scope();
        self.ret_stack.pop();
        self.loop_depth = saved_loop;
        Ty::Fn(params, Box::new(ret))
    }

    /// Check a function body against its return type.
    pub fn check_body_result(&mut self, body: &Block, ret: &Ty, fn_span: Span, what: &str) {
        let want = if *ret == Ty::Unit {
            Want::Unused
        } else {
            Want::Exp(ret.clone())
        };
        let t = self.check_block(body, &want);
        if *ret == Ty::Unit {
            return;
        }
        if self.assignable(&t, ret) {
            return;
        }
        match body.stmts.last() {
            Some(Stmt {
                kind: StmtKind::Expr(e),
                ..
            }) if t != Ty::Unit => self.mismatch(e.span, ret, &t),
            _ => {
                let r = self.show(ret);
                self.err_note(
                    "T181",
                    format!("{what} must give a value of type `{r}`"),
                    fn_span,
                    "the last line of the body is the value of the function (or use `return`)",
                    body.stmts.last().map(|s| s.span),
                );
            }
        }
    }

    // ------------------------------------------------------- struct literals

    fn check_struct_lit(&mut self, e: &Expr, lit: &StructLit, want: &Want) -> Ty {
        let Some(item) = self.lookup_path_item(&lit.path, "type") else {
            for f in &lit.fields {
                self.expr(&f.value, &Want::Any);
            }
            return Ty::Error;
        };
        let ItemRef::Struct(sid) = item else {
            let n = lit.path.last().unwrap().name.clone();
            self.err("T230", format!("`{n}` is not a struct"), e.span);
            return Ty::Error;
        };
        let sdef = self.defs.structs[sid].clone();
        let args: Vec<Ty> = if !lit.targs.is_empty() {
            if lit.targs.len() != sdef.tparams.len() {
                self.err(
                    "T121",
                    format!(
                        "`{}` expects {} type argument(s)",
                        sdef.name,
                        sdef.tparams.len()
                    ),
                    e.span,
                );
                return Ty::Error;
            }
            lit.targs.iter().map(|t| self.resolve_type(t)).collect()
        } else {
            sdef.tparams.iter().map(|_| self.fresh()).collect()
        };
        let st = Ty::Struct(sid, args.clone());
        if let Some(Ty::Struct(wid, wargs)) = want.expected().map(|t| self.resolve(t))
            && wid == sid
        {
            for (a, b) in args.iter().zip(wargs.iter()) {
                self.same(a, b);
            }
        }
        // all instance fields, own and inherited
        struct F {
            ty: Ty,
            vis: Visibility,
            owner: StructId,
            has_default: bool,
        }
        let mut fields: Vec<(String, F)> = Vec::new();
        for (asid, aargs) in self.defs.ancestry(sid, &args) {
            let ad = self.defs.structs[asid].clone();
            let map: HashMap<ParamId, Ty> = ad
                .tparams
                .iter()
                .cloned()
                .zip(aargs.iter().cloned())
                .collect();
            for f in &ad.fields {
                if f.is_static {
                    continue;
                }
                fields.push((
                    f.name.clone(),
                    F {
                        ty: f.ty.subst(&map),
                        vis: f.vis,
                        owner: asid,
                        has_default: f.default.is_some(),
                    },
                ));
            }
        }
        let mut covered: HashSet<String> = HashSet::new();
        if let Some(b) = &lit.base {
            let bt = self.expr(b, &Want::Any);
            match self.resolve(&bt) {
                Ty::Struct(bsid, bargs) => match self.ancestor_args(sid, &args, bsid) {
                    Some(want_args) => {
                        for (x, y) in bargs.iter().zip(want_args.iter()) {
                            self.same(x, y);
                        }
                        for (asid, _) in self.defs.ancestry(bsid, &bargs) {
                            for f in &self.defs.structs[asid].fields {
                                if !f.is_static {
                                    covered.insert(f.name.clone());
                                }
                            }
                        }
                    }
                    None => {
                        let (a, b2) = (self.show(&bt), self.show(&st));
                        self.err("T231", format!("`..base` must be a value of `{b2}` or of one of its parent structs, found `{a}`"), b.span);
                    }
                },
                Ty::Error => {}
                other => {
                    let shown = other.show(&self.defs);
                    self.err(
                        "T231",
                        format!("`..base` must be a struct value, found `{shown}`"),
                        b.span,
                    );
                }
            }
        }
        let mut given: HashSet<String> = HashSet::new();
        for item in &lit.fields {
            let Some((_, f)) = fields.iter().find(|(n, _)| *n == item.name.name) else {
                let in_static = self.defs.ancestry(sid, &args).iter().any(|(a, _)| {
                    self.defs.structs[*a]
                        .fields
                        .iter()
                        .any(|f| f.is_static && f.name == item.name.name)
                });
                let msg = if in_static {
                    format!(
                        "`{}` is a static field: it belongs to the type, not to each value",
                        item.name.name
                    )
                } else {
                    format!("struct `{}` has no field `{}`", sdef.name, item.name.name)
                };
                self.err("T232", msg, item.name.span);
                self.expr(&item.value, &Want::Any);
                continue;
            };
            let (fty, fvis, fowner) = (f.ty.clone(), f.vis, f.owner);
            if !given.insert(item.name.name.clone()) {
                self.err(
                    "T233",
                    format!("field `{}` is given twice", item.name.name),
                    item.name.span,
                );
            }
            self.check_vis(
                fvis,
                fowner,
                item.name.span,
                &format!("field `{}`", item.name.name),
            );
            let vt = self.expr(&item.value, &Want::Exp(fty.clone()));
            if !self.assignable(&vt, &fty) {
                self.mismatch(item.value.span, &fty, &vt);
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|(n, f)| !given.contains(n) && !covered.contains(n) && !f.has_default)
            .map(|(n, _)| format!("`{n}`"))
            .collect();
        if !missing.is_empty() {
            self.err(
                "T234",
                format!(
                    "missing field(s) for `{}`: {}",
                    sdef.name,
                    missing.join(", ")
                ),
                e.span,
            );
        }
        self.res.struct_lits.insert(e.id, sid);
        let st = self.resolve(&st);
        if st.contains_infer() {
            let n = sdef.name.clone();
            self.err(
                "T171",
                format!("cannot infer the type arguments of `{n}`: write `{n}<...> {{ ... }}`"),
                e.span,
            );
            return Ty::Error;
        }
        st
    }
}
