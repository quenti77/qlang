//! Checking the bodies: top-level statements, field defaults, functions.

use super::expr::Want;
use super::*;
use crate::ast::{Ident, Item};

impl Checker {
    pub(super) fn reset_body_state(&mut self, m: ModId) {
        self.cur_mod = m;
        self.scopes.clear();
        self.tparam_scope.clear();
        self.ret_stack.clear();
        self.loop_depth = 0;
        self.cur_impl_struct = None;
        self.cur_self = None;
        self.cur_fn = None;
    }

    pub(crate) fn check_module_body(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        self.reset_body_state(m);
        self.check_annotations(m, &ast);
        self.reset_body_state(m);

        // top-level statements, in order
        for item in &ast.items {
            if let Item::Stmt(s) = item {
                self.check_stmt(s, &Want::Unused);
            }
        }

        // field defaults
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Struct(s) = item else { continue };
            let sid = self.mscopes[m].decl_ids[&idx];
            for f in &s.fields {
                let Some(d) = &f.default else { continue };
                self.reset_body_state(m);
                self.tparam_scope = s
                    .tparams
                    .iter()
                    .zip(self.defs.structs[sid].tparams.iter())
                    .map(|(t, p)| (t.name.name.clone(), *p))
                    .collect();
                self.cur_impl_struct = Some(sid);
                let fty = self.defs.structs[sid]
                    .fields
                    .iter()
                    .find(|x| x.name == f.name.name)
                    .map(|x| x.ty.clone())
                    .unwrap_or(Ty::Error);
                let t = self.expr(d, &Want::Exp(fty.clone()));
                if !self.assignable(&t, &fty) {
                    self.mismatch(d.span, &fty, &t);
                }
            }
        }

        // functions and methods
        for (idx, item) in ast.items.iter().enumerate() {
            match item {
                Item::Fun(_) => {
                    let fid = self.mscopes[m].decl_ids[&idx];
                    self.check_fn_body(fid);
                }
                Item::Impl(_) => {
                    if let Some(&iid) = self.mscopes[m].decl_ids.get(&idx) {
                        // trait defaults used to fill a gap are checked once, below,
                        // not once per impl that happens to fall back to them.
                        let fids: Vec<FnId> = self.defs.impls[iid]
                            .methods
                            .iter()
                            .map(|(_, f)| *f)
                            .filter(|f| self.defs.fns[*f].trait_default_self.is_none())
                            .collect();
                        for fid in fids {
                            self.check_fn_body(fid);
                        }
                    }
                }
                Item::Trait(_) => {
                    if let Some(&tid) = self.mscopes[m].decl_ids.get(&idx) {
                        let fids: Vec<FnId> = self.defs.traits[tid]
                            .sigs
                            .iter()
                            .filter_map(|s| s.default)
                            .collect();
                        for fid in fids {
                            self.check_fn_body(fid);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_fn_body(&mut self, fid: FnId) {
        let def = self.defs.fns[fid].clone();
        let Some(decl) = def.decl.clone() else { return };
        let Some(body) = &decl.body else { return };
        self.reset_body_state(def.module);
        self.tparam_scope = def.scope_params.clone();
        self.cur_fn = Some(fid);
        self.scopes.push(HashMap::new());
        if let Some(iid) = def.impl_id {
            let target = self.defs.impls[iid].target.clone();
            if let Ty::Struct(sid, _) = &target {
                self.cur_impl_struct = Some(*sid);
            }
            if def.has_self {
                self.cur_self = Some(target.clone());
                let name = Ident {
                    name: "self".to_string(),
                    span: decl.span,
                };
                self.declare(
                    &name,
                    Local {
                        ty: target,
                        kind: LocalKind::Param,
                        orig: None,
                        from_module: false,
                        span: decl.span,
                    },
                );
            }
        } else if let Some(self_ty) = def.trait_default_self.clone()
            && def.has_self
        {
            self.cur_self = Some(self_ty.clone());
            let name = Ident {
                name: "self".to_string(),
                span: decl.span,
            };
            self.declare(
                &name,
                Local {
                    ty: self_ty,
                    kind: LocalKind::Param,
                    orig: None,
                    from_module: false,
                    span: decl.span,
                },
            );
        }
        for (p, t) in decl.params.iter().zip(def.params.iter()) {
            self.declare(
                &p.name,
                Local {
                    ty: t.clone(),
                    kind: LocalKind::Param,
                    orig: None,
                    from_module: false,
                    span: p.name.span,
                },
            );
        }
        self.ret_stack.push(def.ret.clone());
        let span = decl.name.as_ref().map(|n| n.span).unwrap_or(decl.span);
        let what = format!("function `{}`", def.name);
        self.check_body_result(body, &def.ret, span, &what);
        self.scopes.pop();
    }
}

impl Checker {
    /// The types recorded for the typed machine are resolved now that inference is over; a
    /// type that is still open is dropped (the machine does not compile that expression).
    pub(crate) fn finish_expr_tys(&mut self) {
        let tys: Vec<(crate::ast::NodeId, Ty)> = self.res.expr_tys.drain().collect();
        for (id, t) in tys {
            let t = self.resolve(&t);
            if !t.contains_infer() && !t.contains_param() {
                self.res.expr_tys.insert(id, t);
            }
        }
    }
}
