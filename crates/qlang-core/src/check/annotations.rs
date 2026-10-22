//! Annotations (`#[Route("/users", method: "GET")]`): each one names a struct declared with
//! `#[annotation]`, and its arguments are literals that fit the struct's fields.

use super::expr::Want;
use super::*;
use crate::ast::{Annotation, ExprKind, Item, Module, StrPart, UnOp};

impl Checker {
    pub(crate) fn check_annotations(&mut self, ast: &Module) {
        for item in ast.items.iter() {
            match item {
                Item::Fun(f) => self.check_fun_annotations(f),
                Item::Struct(s) => {
                    self.check_annotation_list(&s.annotations, true);
                    if s.annotations.iter().any(is_marker) {
                        self.check_annotation_struct(s);
                    }
                    for f in &s.fields {
                        self.check_annotation_list(&f.annotations, false);
                    }
                }
                Item::Trait(t) => {
                    self.check_annotation_list(&t.annotations, false);
                    for sig in &t.sigs {
                        self.check_fun_annotations(sig);
                    }
                }
                Item::Enum(e) => {
                    self.check_annotation_list(&e.annotations, false);
                    for v in &e.variants {
                        self.check_annotation_list(&v.annotations, false);
                    }
                }
                Item::Impl(i) => {
                    for md in &i.methods {
                        self.check_fun_annotations(&md.fun);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_fun_annotations(&mut self, f: &ast::FunDecl) {
        self.check_annotation_list(&f.annotations, false);
        for p in &f.params {
            self.check_annotation_list(&p.annotations, false);
        }
    }

    /// An annotation struct is plain data: no type parameters, no parent, no static field.
    fn check_annotation_struct(&mut self, s: &ast::StructDecl) {
        if !s.tparams.is_empty() || s.parent.is_some() || s.fields.iter().any(|f| f.is_static) {
            self.err(
                "T295",
                format!(
                    "`{}` is an annotation: it cannot have type parameters, a parent or static fields",
                    s.name.name
                ),
                s.name.span,
            );
        }
    }

    /// `on_struct`: the declaration is a struct (where `#[annotation]` is allowed).
    fn check_annotation_list(&mut self, list: &[Annotation], on_struct: bool) {
        for a in list {
            if is_marker(a) {
                if !on_struct {
                    self.err("T291", "`#[annotation]` only applies to a `struct`", a.span);
                } else if !a.args.is_empty() {
                    self.err("T291", "`#[annotation]` takes no arguments", a.span);
                }
                continue;
            }
            self.check_annotation(a);
        }
    }

    fn check_annotation(&mut self, a: &Annotation) {
        let Some(item) = self.lookup_path_item(&a.path, "annotation") else {
            return;
        };
        let name = a.path.last().map(|i| i.name.as_str()).unwrap_or("");
        let sid = match item {
            ItemRef::Struct(sid) if self.defs.structs[sid].annotation => sid,
            _ => {
                self.err(
                    "T290",
                    format!(
                        "`{name}` is not an annotation (declare it with `#[annotation]` on a `struct`)"
                    ),
                    a.path[0].span,
                );
                return;
            }
        };
        let fields: Vec<(String, Ty, bool)> = self.defs.structs[sid]
            .fields
            .iter()
            .filter(|f| !f.is_static)
            .map(|f| (f.name.clone(), f.ty.clone(), f.default.is_some()))
            .collect();
        let errors_before = self.diags.len();
        let mut given: Vec<bool> = vec![false; fields.len()];
        let mut next = 0usize;
        for arg in &a.args {
            if !is_literal(&arg.value) {
                self.err(
                    "T294",
                    "an annotation argument must be a literal (a number, a text, `true`, `false`, `none` or an array of those)",
                    arg.value.span,
                );
                continue;
            }
            let slot = match &arg.name {
                Some(n) => match fields.iter().position(|f| f.0 == n.name) {
                    Some(i) => i,
                    None => {
                        self.err(
                            "T292",
                            format!("`{name}` has no field `{}`", n.name),
                            n.span,
                        );
                        continue;
                    }
                },
                None => {
                    while next < fields.len() && given[next] {
                        next += 1;
                    }
                    if next >= fields.len() {
                        self.err(
                            "T292",
                            format!("`{name}` takes {} argument(s)", fields.len()),
                            arg.value.span,
                        );
                        continue;
                    }
                    next
                }
            };
            if given[slot] {
                self.err(
                    "T292",
                    format!("field `{}` of `{name}` is given twice", fields[slot].0),
                    arg.value.span,
                );
                continue;
            }
            given[slot] = true;
            let fty = fields[slot].1.clone();
            let t = self.expr(&arg.value, &Want::Exp(fty.clone()));
            if !self.assignable(&t, &fty) {
                self.mismatch(arg.value.span, &fty, &t);
            }
        }
        // an argument that was refused may be the one that is "missing"
        if self.diags.len() > errors_before {
            return;
        }
        for (i, (fname, _, has_default)) in fields.iter().enumerate() {
            if !given[i] && !has_default {
                self.err(
                    "T293",
                    format!("`{name}` needs a value for `{fname}`"),
                    a.span,
                );
            }
        }
    }
}

fn is_marker(a: &Annotation) -> bool {
    a.path.len() == 1 && a.path[0].name == "annotation"
}

fn is_literal(e: &ast::Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::None => true,
        ExprKind::Str(parts) => parts.iter().all(|p| matches!(p, StrPart::Lit(_))),
        ExprKind::Unary(UnOp::Neg, inner) => {
            matches!(inner.kind, ExprKind::Int(_) | ExprKind::Float(_))
        }
        ExprKind::Array(items) => items.iter().all(is_literal),
        _ => false,
    }
}
