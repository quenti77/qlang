//! Annotations (`#[Route("/users", method: "GET")]`): each one names a struct declared with
//! `#[annotation]`, and its arguments are literals that fit the struct's fields.

use super::expr::Want;
use super::*;
use crate::ast::{Annotation, ExprKind, Item, Module, StrPart, UnOp};

impl Checker {
    pub(crate) fn check_annotations(&mut self, m: ModId, ast: &Module) {
        for (idx, item) in ast.items.iter().enumerate() {
            match item {
                Item::Fun(f) => {
                    let fid = self.mscopes[m].decl_ids[&idx];
                    self.check_fun_annotations(f, Some(fid));
                }
                Item::Struct(s) => {
                    let sid = self.mscopes[m].decl_ids[&idx];
                    self.check_annotation_list(&s.annotations, true, Some(AnnKey::Struct(sid)));
                    if s.annotations.iter().any(is_marker) {
                        self.check_annotation_struct(s);
                    }
                    for (i, f) in s.fields.iter().enumerate() {
                        self.check_annotation_list(
                            &f.annotations,
                            false,
                            Some(AnnKey::Field(sid, i)),
                        );
                    }
                }
                Item::Trait(t) => {
                    self.check_annotation_list(&t.annotations, false, None);
                    for sig in &t.sigs {
                        self.check_fun_annotations(sig, None);
                    }
                }
                Item::Enum(e) => {
                    let eid = self.mscopes[m].decl_ids[&idx];
                    self.check_annotation_list(&e.annotations, false, Some(AnnKey::Enum(eid)));
                    for (i, v) in e.variants.iter().enumerate() {
                        self.check_annotation_list(
                            &v.annotations,
                            false,
                            Some(AnnKey::Variant(eid, i)),
                        );
                    }
                }
                Item::Impl(i) => {
                    let impl_id = self.mscopes[m].decl_ids.get(&idx).copied();
                    for md in &i.methods {
                        let fid = impl_id.and_then(|iid| {
                            let name = md.fun.name.as_ref()?;
                            let methods = &self.defs.impls[iid].methods;
                            methods
                                .iter()
                                .find(|(n, _)| *n == name.name)
                                .map(|(_, f)| *f)
                        });
                        self.check_fun_annotations(&md.fun, fid);
                    }
                }
                _ => {}
            }
        }
    }

    /// `fid`: the function, for the declarations reflection reaches (not methods).
    fn check_fun_annotations(&mut self, f: &ast::FunDecl, fid: Option<FnId>) {
        self.check_annotation_list(&f.annotations, false, fid.map(AnnKey::Fn));
        for (i, p) in f.params.iter().enumerate() {
            self.check_annotation_list(&p.annotations, false, fid.map(|f| AnnKey::Param(f, i)));
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
    fn check_annotation_list(&mut self, list: &[Annotation], on_struct: bool, key: Option<AnnKey>) {
        for a in list {
            if is_marker(a) {
                if !on_struct {
                    self.err("T291", "`#[annotation]` only applies to a `struct`", a.span);
                } else if !a.args.is_empty() {
                    self.err("T291", "`#[annotation]` takes no arguments", a.span);
                }
                continue;
            }
            if let (Some(data), Some(key)) = (self.check_annotation(a), key) {
                self.res.annotations.entry(key).or_default().push(data);
            }
        }
    }

    fn check_annotation(&mut self, a: &Annotation) -> Option<AnnData> {
        let item = self.lookup_path_item(&a.path, "annotation")?;
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
                return None;
            }
        };
        let fields: Vec<(String, Ty, Option<Rc<ast::Expr>>)> = self.defs.structs[sid]
            .fields
            .iter()
            .filter(|f| !f.is_static)
            .map(|f| (f.name.clone(), f.ty.clone(), f.default.clone()))
            .collect();
        let errors_before = self.diags.len();
        let mut given: Vec<bool> = vec![false; fields.len()];
        let mut values: Vec<Option<Lit>> = vec![None; fields.len()];
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
            values[slot] = literal_of(&arg.value);
            let fty = fields[slot].1.clone();
            // `#[Min(1)]` for a `float` field: a whole-number literal is fine there
            if fty == Ty::Float
                && let Some(Lit::Int(i)) = values[slot]
            {
                values[slot] = Some(Lit::Float(i as f64));
                self.expr(&arg.value, &Want::Any);
                continue;
            }
            let t = self.expr(&arg.value, &Want::Exp(fty.clone()));
            if !self.assignable(&t, &fty) {
                self.mismatch(arg.value.span, &fty, &t);
            }
        }
        // an argument that was refused may be the one that is "missing"
        if self.diags.len() > errors_before {
            return None;
        }
        let mut missing = false;
        for (i, (fname, _, default)) in fields.iter().enumerate() {
            if !given[i] && default.is_none() {
                missing = true;
                self.err(
                    "T293",
                    format!("`{name}` needs a value for `{fname}`"),
                    a.span,
                );
            }
        }
        if missing {
            return None;
        }
        // the arguments given, and the defaults of the others (when they are literals)
        let args = fields
            .iter()
            .zip(values)
            .filter_map(|((fname, _, default), v)| {
                let v = v.or_else(|| default.as_ref().and_then(|d| literal_of(d)))?;
                Some((fname.clone(), v))
            })
            .collect();
        Some(AnnData {
            name: name.to_string(),
            args,
        })
    }
}

/// The value of a literal expression (see `is_literal`).
fn literal_of(e: &ast::Expr) -> Option<Lit> {
    Some(match &e.kind {
        ExprKind::Int(i) => Lit::Int(*i),
        ExprKind::Float(f) => Lit::Float(*f),
        ExprKind::Bool(b) => Lit::Bool(*b),
        ExprKind::None => Lit::None,
        ExprKind::Str(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    StrPart::Lit(t) => out.push_str(t),
                    StrPart::Expr(_) => return None,
                }
            }
            Lit::Str(out)
        }
        ExprKind::Unary(UnOp::Neg, inner) => match &inner.kind {
            ExprKind::Int(i) => Lit::Int(i.wrapping_neg()),
            ExprKind::Float(f) => Lit::Float(-f),
            _ => return None,
        },
        ExprKind::Array(items) => Lit::Array(items.iter().map(literal_of).collect::<Option<_>>()?),
        _ => return None,
    })
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
