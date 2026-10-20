//! Which variables a function or lambda assigns without having declared them.
//!
//! `if x != none then ... end` shows `x` as non-`none` inside the `if`. That is only
//! sound if nothing can set `x` back to `none` in between, and a call to a function (or a
//! lambda) that assigns to `x` can. So variables assigned from inside another function
//! are never narrowed. This is a syntactic approximation by name: a name declared in the
//! function itself (parameter, `let`, `for` variable, `match` binding) is its own.

use crate::ast::*;
use std::collections::HashSet;

/// The names assigned from inside a function or lambda of `module` that are not that
/// function's own.
pub fn assigned_from_closures(module: &Module) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in &module.items {
        match item {
            Item::Fun(f) => function(f, &mut out),
            Item::Impl(i) => i.methods.iter().for_each(|m| function(&m.fun, &mut out)),
            Item::Trait(t) => t.sigs.iter().for_each(|f| function(f, &mut out)),
            Item::Struct(s) => {
                for d in s.fields.iter().filter_map(|f| f.default.as_ref()) {
                    let mut scope = Scope::default();
                    scope.expr(d, &mut out);
                }
            }
            Item::Stmt(s) => Scope::default().stmt(s, &mut out),
            Item::Import(_) | Item::Export(_) | Item::Enum(_) => {}
        }
    }
    out
}

/// What one function body declares and assigns.
#[derive(Default)]
struct Scope {
    declared: HashSet<String>,
    assigned: HashSet<String>,
}

fn function(f: &FunDecl, out: &mut HashSet<String>) {
    let mut scope = Scope::default();
    scope
        .declared
        .extend(f.params.iter().map(|p| p.name.name.clone()));
    if let Some(body) = &f.body {
        scope.block(body, out);
    }
    out.extend(scope.assigned.difference(&scope.declared).cloned());
}

impl Scope {
    fn block(&mut self, b: &Block, out: &mut HashSet<String>) {
        for s in &b.stmts {
            self.stmt(s, out);
        }
    }

    fn stmt(&mut self, s: &Stmt, out: &mut HashSet<String>) {
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                self.declared.insert(name.name.clone());
                self.expr(init, out);
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.expr(e, out);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, out);
                self.block(body, out);
            }
            StmtKind::For {
                var,
                var2,
                iter,
                step,
                body,
            } => {
                self.declared.insert(var.name.clone());
                if let Some(v) = var2 {
                    self.declared.insert(v.name.clone());
                }
                self.expr(iter, out);
                if let Some(e) = step {
                    self.expr(e, out);
                }
                self.block(body, out);
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(e) => self.expr(e, out),
        }
    }

    fn expr(&mut self, e: &Expr, out: &mut HashSet<String>) {
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Ident(..)
            | ExprKind::SelfVal
            | ExprKind::Super => {}
            ExprKind::Str(parts) => {
                for p in parts {
                    if let StrPart::Expr(x) = p {
                        self.expr(x, out);
                    }
                }
            }
            ExprKind::Array(xs) => xs.iter().for_each(|x| self.expr(x, out)),
            ExprKind::Map(kvs) => {
                for (k, v) in kvs {
                    self.expr(k, out);
                    self.expr(v, out);
                }
            }
            ExprKind::StructLit(lit) => {
                if let Some(b) = &lit.base {
                    self.expr(b, out);
                }
                lit.fields.iter().for_each(|f| self.expr(&f.value, out));
            }
            ExprKind::Unary(_, x) | ExprKind::Cast(x, _) | ExprKind::Append(x) => self.expr(x, out),
            ExprKind::Field(x, ..) => self.expr(x, out),
            ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) => {
                self.expr(a, out);
                self.expr(b, out);
            }
            ExprKind::Assign { target, value, .. } => {
                if let ExprKind::Ident(name, _) = &target.kind {
                    self.assigned.insert(name.clone());
                }
                self.expr(target, out);
                self.expr(value, out);
            }
            ExprKind::Range { start, end, .. } => {
                self.expr(start, out);
                self.expr(end, out);
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee, out);
                args.iter().for_each(|a| self.expr(&a.value, out));
            }
            ExprKind::If {
                branches,
                else_block,
            } => {
                for (cond, body) in branches {
                    self.expr(cond, out);
                    self.block(body, out);
                }
                if let Some(b) = else_block {
                    self.block(b, out);
                }
            }
            ExprKind::Match {
                scrutinee,
                cases,
                else_block,
            } => {
                self.expr(scrutinee, out);
                for c in cases {
                    if let Pattern::Bind(name) = &c.pattern {
                        self.declared.insert(name.name.clone());
                    }
                    if let Some(g) = &c.guard {
                        self.expr(g, out);
                    }
                    self.block(&c.body, out);
                }
                if let Some(b) = else_block {
                    self.block(b, out);
                }
            }
            // a nested lambda is a function of its own; what it assigns and does not
            // declare is added to `out` (it may be a variable of the enclosing function)
            ExprKind::Lambda(f) => function(f, out),
        }
    }
}
