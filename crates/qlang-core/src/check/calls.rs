//! Calls, field access, methods, static members, built-in functions.

use super::expr::Want;
use super::*;
use crate::ast::*;

/// A callable signature, before instantiating its generics.
pub(crate) struct CallSig {
    pub name: String,
    /// Generic parameters to infer (or take from explicit arguments).
    pub tparams: Vec<ParamId>,
    pub params: Vec<Ty>,
    pub ret: Ty,
}

pub(crate) enum Head {
    /// A type used as a namespace: `Point`, `Color`, `int`.
    Type(Ty, TypeRef),
    Module(ModId),
    /// An error was already reported.
    Error,
}

impl Checker {
    // ------------------------------------------------------------- name heads

    /// If `e` names a type or a module (and not a variable), say which.
    pub fn try_head(&mut self, e: &Expr) -> Option<Head> {
        match &e.kind {
            ExprKind::Ident(name, targs) => {
                if self.lookup_var(name).is_some() {
                    return None;
                }
                match self.lookup_item(name) {
                    Some(ItemRef::Module(m)) => {
                        self.res.paths.insert(e.id, PathRes::Module(m));
                        Some(Head::Module(m))
                    }
                    Some(item @ (ItemRef::Struct(_) | ItemRef::Enum(_))) => Some(self.type_head(e, item, targs)),
                    Some(_) => None,
                    None => {
                        let (ty, b) = match name.as_str() {
                            "int" => (Ty::Int, "int"),
                            "float" => (Ty::Float, "float"),
                            "bool" => (Ty::Bool, "bool"),
                            "string" => (Ty::Str, "string"),
                            "array" => {
                                let f = self.fresh();
                                (Ty::Array(Box::new(f)), "array")
                            }
                            _ => return None,
                        };
                        self.res.paths.insert(e.id, PathRes::Type(TypeRef::Builtin(b)));
                        Some(Head::Type(ty, TypeRef::Builtin(b)))
                    }
                }
            }
            ExprKind::Field(obj, name, targs) => match self.try_head(obj)? {
                Head::Module(m) => {
                    let Some(item) = self.module_member(m, name) else { return Some(Head::Error) };
                    match item {
                        ItemRef::Module(m2) => {
                            self.res.paths.insert(e.id, PathRes::Module(m2));
                            Some(Head::Module(m2))
                        }
                        ItemRef::Struct(_) | ItemRef::Enum(_) => Some(self.type_head(e, item, targs)),
                        _ => None,
                    }
                }
                Head::Error => Some(Head::Error),
                Head::Type(..) => None,
            },
            _ => None,
        }
    }

    fn type_head(&mut self, e: &Expr, item: ItemRef, targs: &[TypeExpr]) -> Head {
        match item {
            ItemRef::Struct(id) => {
                let n = self.defs.structs[id].tparams.len();
                let args: Vec<Ty> = if targs.is_empty() {
                    (0..n).map(|_| self.fresh()).collect()
                } else if targs.len() == n {
                    targs.iter().map(|t| self.resolve_type(t)).collect()
                } else {
                    self.err("T121", format!("`{}` expects {n} type argument(s)", self.defs.structs[id].name), e.span);
                    return Head::Error;
                };
                self.res.paths.insert(e.id, PathRes::Type(TypeRef::Struct(id)));
                Head::Type(Ty::Struct(id, args), TypeRef::Struct(id))
            }
            ItemRef::Enum(id) => {
                self.res.paths.insert(e.id, PathRes::Type(TypeRef::Enum(id)));
                Head::Type(Ty::Enum(id), TypeRef::Enum(id))
            }
            _ => Head::Error,
        }
    }

    // ------------------------------------------------------------ field access

    pub fn check_field(&mut self, e: &Expr, obj: &Expr, name: &Ident, targs: &[TypeExpr]) -> Ty {
        match self.try_head(obj) {
            Some(Head::Error) => Ty::Error,
            Some(Head::Module(m)) => {
                let Some(item) = self.module_member(m, name) else { return Ty::Error };
                match item {
                    ItemRef::Fn(fid) => self.fn_value(e, fid, targs),
                    ItemRef::Var(m2, vn) => match self.mscopes[m2].vars.get(&vn).cloned() {
                        Some(l) => {
                            self.res.paths.insert(e.id, PathRes::Global(m2, vn));
                            l.ty
                        }
                        None => Ty::Error,
                    },
                    _ => {
                        self.err("T154", format!("`{}` is a type or a module, not a value", name.name), e.span);
                        Ty::Error
                    }
                }
            }
            Some(Head::Type(ty, tref)) => self.static_member_value(e, &ty, &tref, name, targs),
            None => {
                let ot = self.expr(obj, &Want::Any);
                let is_var = matches!(obj.kind, ExprKind::Ident(..) | ExprKind::SelfVal);
                self.field_of(e, &ot, name, is_var)
            }
        }
    }

    pub fn check_field_target(&mut self, target: &Expr, obj: &Expr, name: &Ident, targs: &[TypeExpr]) -> Ty {
        let t = self.check_field(target, obj, name, targs);
        if matches!(self.res.paths.get(&target.id), Some(PathRes::EnumVariant(..) | PathRes::Fn(_) | PathRes::Module(_))) {
            self.err("T212", format!("cannot assign to `{}`", name.name), target.span);
        }
        t
    }

    fn static_member_value(&mut self, e: &Expr, ty: &Ty, tref: &TypeRef, name: &Ident, targs: &[TypeExpr]) -> Ty {
        match tref {
            TypeRef::Enum(eid) => {
                if let Some(vi) = self.defs.enums[*eid].variants.iter().position(|v| *v == name.name) {
                    self.res.paths.insert(e.id, PathRes::EnumVariant(*eid, vi));
                    return Ty::Enum(*eid);
                }
            }
            TypeRef::Struct(sid) => {
                let Ty::Struct(_, args) = ty else { return Ty::Error };
                for (asid, aargs) in self.defs.ancestry(*sid, args) {
                    let ad = self.defs.structs[asid].clone();
                    if let Some(f) = ad.fields.iter().find(|f| f.is_static && f.name == name.name) {
                        let map: HashMap<ParamId, Ty> = ad.tparams.iter().cloned().zip(aargs.iter().cloned()).collect();
                        self.check_vis(f.vis, asid, name.span, &format!("static field `{}`", name.name));
                        self.res.paths.insert(e.id, PathRes::StaticField(asid, name.name.clone()));
                        return f.ty.subst(&map);
                    }
                }
            }
            TypeRef::Builtin(b) => {
                let found = match (*b, name.name.as_str()) {
                    ("float", "PI") => Some(("float", "PI", Ty::Float)),
                    ("float", "E") => Some(("float", "E", Ty::Float)),
                    ("int", "MAX") => Some(("int", "MAX", Ty::Int)),
                    ("int", "MIN") => Some(("int", "MIN", Ty::Int)),
                    _ => None,
                };
                if let Some((a, n, t)) = found {
                    self.res.paths.insert(e.id, PathRes::BuiltinStatic(a, n));
                    return t;
                }
            }
        }
        if let Some(um) = self.find_user_method(ty, &name.name) {
            let f = self.defs.fns[um.fid].clone();
            if !f.has_self {
                self.res.paths.insert(e.id, PathRes::Fn(um.fid));
                let _ = targs;
                return Ty::Fn(f.params.iter().map(|p| p.subst(&um.map)).collect(), Box::new(f.ret.subst(&um.map)));
            }
        }
        let shown = self.show(ty);
        self.err("T161", format!("`{shown}` has no static member `{}`", name.name), name.span);
        Ty::Error
    }

    pub fn field_of(&mut self, e: &Expr, ot: &Ty, name: &Ident, obj_is_variable: bool) -> Ty {
        let ot = self.resolve(ot);
        match &ot {
            Ty::Error => Ty::Error,
            Ty::Nullable(_) | Ty::NoneT => {
                let shown = self.show(&ot);
                let hint = if obj_is_variable { "" } else { " (a field or a call cannot be checked in place: copy it into a variable first, then test the variable)" };
                self.err(
                    "T162",
                    format!("this value may be `none` (`{shown}`): check `!= none` before using `.{}`{hint}", name.name),
                    e.span,
                );
                Ty::Error
            }
            Ty::Struct(sid, args) => {
                for (asid, aargs) in self.defs.ancestry(*sid, args) {
                    let ad = self.defs.structs[asid].clone();
                    if let Some(f) = ad.fields.iter().find(|f| f.name == name.name) {
                        if f.is_static {
                            self.err(
                                "T163",
                                format!("`{}` is a static field: use `{}.{}`", name.name, ad.name, name.name),
                                name.span,
                            );
                            return Ty::Error;
                        }
                        let map: HashMap<ParamId, Ty> = ad.tparams.iter().cloned().zip(aargs.iter().cloned()).collect();
                        self.check_vis(f.vis, asid, name.span, &format!("field `{}`", name.name));
                        return f.ty.subst(&map);
                    }
                }
                let is_method = self.find_user_method(&ot, &name.name).is_some();
                let shown = self.show(&ot);
                if is_method {
                    self.err("T164", format!("`{}` is a method of `{shown}`: call it with `()`", name.name), name.span);
                } else {
                    self.err("T161", format!("`{shown}` has no field `{}`", name.name), name.span);
                }
                Ty::Error
            }
            other => {
                let shown = other.show(&self.defs);
                if self.builtin_method(other, &name.name).is_some() {
                    self.err("T164", format!("`{}` is a method of `{shown}`: call it with `()`", name.name), name.span);
                } else {
                    self.err("T161", format!("`{shown}` has no field `{}`", name.name), name.span);
                }
                Ty::Error
            }
        }
    }

    // ------------------------------------------------------------------ calls

    /// Check arguments loosely after an error, to still report problems inside them.
    fn check_args_loose(&mut self, args: &[Expr]) {
        for a in args {
            self.expr(a, &Want::Any);
        }
    }

    pub fn check_call(&mut self, e: &Expr, callee: &Expr, args: &[Expr], _want: &Want) -> Ty {
        match &callee.kind {
            ExprKind::Field(obj, name, targs) => {
                if matches!(obj.kind, ExprKind::Super) {
                    return self.call_super(e, callee, name, args);
                }
                match self.try_head(obj) {
                    Some(Head::Error) => {
                        self.check_args_loose(args);
                        return Ty::Error;
                    }
                    Some(Head::Module(m)) => {
                        let Some(item) = self.module_member(m, name) else {
                            self.check_args_loose(args);
                            return Ty::Error;
                        };
                        match item {
                            ItemRef::Fn(fid) => return self.call_fn_item(e, callee, fid, targs, args),
                            ItemRef::Struct(_) => {
                                self.err("T154", format!("`{0}` is a type: use `{0}.new(...)` or `{0} {{ ... }}`", name.name), callee.span);
                                self.check_args_loose(args);
                                return Ty::Error;
                            }
                            _ => {}
                        }
                        // a variable holding a function: fall through to a value call
                    }
                    Some(Head::Type(ty, tref)) => return self.call_static(e, callee, &ty, &tref, name, targs, args),
                    None => {
                        let rt = self.expr(obj, &Want::Any);
                        return self.call_method(e, callee, &rt, name, targs, args);
                    }
                }
            }
            ExprKind::Ident(name, targs)
                if self.lookup_var(name).is_none() => {
                    match self.lookup_item(name) {
                        Some(ItemRef::Fn(fid)) => return self.call_fn_item(e, callee, fid, targs, args),
                        Some(ItemRef::Struct(_) | ItemRef::Enum(_)) => {
                            self.err(
                                "T154",
                                format!("`{name}` is a type, not a function: use `{name}.new(...)` or `{name} {{ ... }}`"),
                                callee.span,
                            );
                            self.check_args_loose(args);
                            return Ty::Error;
                        }
                        None if decls::BUILTIN_FNS.contains(&name.as_str()) => return self.call_builtin_fn(e, callee, name, args),
                        _ => {}
                    }
                }
            _ => {}
        }
        // call a value: a variable or an expression of function type
        let ct = self.expr(callee, &Want::Any);
        match self.resolve(&ct) {
            Ty::Fn(params, ret) => {
                let sig = CallSig { name: callee_name(callee), tparams: vec![], params, ret: *ret };
                self.check_args(e.span, &sig, &[], HashMap::new(), args)
            }
            Ty::Error => {
                self.check_args_loose(args);
                Ty::Error
            }
            other => {
                let shown = other.show(&self.defs);
                self.err("T241", format!("cannot call a `{shown}`: it is not a function"), callee.span);
                self.check_args_loose(args);
                Ty::Error
            }
        }
    }

    fn call_fn_item(&mut self, e: &Expr, callee: &Expr, fid: FnId, targs: &[TypeExpr], args: &[Expr]) -> Ty {
        let def = self.defs.fns[fid].clone();
        self.res.paths.insert(callee.id, PathRes::Fn(fid));
        let own: Vec<ParamId> = def.scope_params.iter().rev().take(def.own_tparams).rev().map(|(_, p)| *p).collect();
        let explicit: Vec<Ty> = targs.iter().map(|t| self.resolve_type(t)).collect();
        let sig = CallSig { name: def.name.clone(), tparams: own, params: def.params.clone(), ret: def.ret.clone() };
        self.check_args(e.span, &sig, &explicit, HashMap::new(), args)
    }

    /// Check arguments against a signature, inferring generic parameters.
    pub fn check_args(&mut self, span: Span, sig: &CallSig, explicit: &[Ty], mut map: HashMap<ParamId, Ty>, args: &[Expr]) -> Ty {
        if !explicit.is_empty() {
            if explicit.len() != sig.tparams.len() {
                self.err("T121", format!("`{}` expects {} type argument(s), found {}", sig.name, sig.tparams.len(), explicit.len()), span);
                self.check_args_loose(args);
                return Ty::Error;
            }
            for (p, t) in sig.tparams.iter().zip(explicit.iter()) {
                map.insert(*p, t.clone());
            }
        } else {
            for p in &sig.tparams {
                let f = self.fresh();
                map.insert(*p, f);
            }
        }
        let params: Vec<Ty> = sig.params.iter().map(|p| p.subst(&map)).collect();
        let ret = sig.ret.subst(&map);
        if args.len() != params.len() {
            self.err(
                "T240",
                format!("`{}` expects {} argument(s), found {}", sig.name, params.len(), args.len()),
                span,
            );
            self.check_args_loose(args);
            return self.resolve(&ret);
        }
        for (a, p) in args.iter().zip(params.iter()) {
            let want = self.resolve(p);
            let t = self.expr(a, &Want::Exp(want.clone()));
            let t = self.require_value(t, a.span);
            if !self.assignable(&t, p) {
                let want = self.resolve(p);
                self.mismatch(a.span, &want, &t);
            }
        }
        for p in &sig.tparams {
            let Some(arg) = map.get(p).cloned() else { continue };
            let arg = self.resolve(&arg);
            if arg.contains_infer() {
                continue;
            }
            for (tid, targs) in self.defs.params[*p].bounds.clone() {
                let targs: Vec<Ty> = targs.iter().map(|t| self.resolve(&t.subst(&map))).collect();
                if !self.implements(&arg, tid, &targs) {
                    let (a, tn) = (self.show(&arg), self.show(&Ty::Trait(tid, targs)));
                    let pn = self.defs.params[*p].name.clone();
                    self.err("T242", format!("`{a}` cannot be used for `{pn}` in `{}`: it does not implement `{tn}`", sig.name), span);
                }
            }
        }
        let ret = self.resolve(&ret);
        if ret.contains_infer() {
            self.err(
                "T171",
                format!("cannot infer the type parameters of `{0}`: write them explicitly, like `{0}<int>(...)`", sig.name),
                span,
            );
            return Ty::Error;
        }
        ret
    }

    fn call_method(&mut self, e: &Expr, callee: &Expr, rt: &Ty, name: &Ident, targs: &[TypeExpr], args: &[Expr]) -> Ty {
        let rt = self.resolve(rt);
        let explicit: Vec<Ty> = targs.iter().map(|t| self.resolve_type(t)).collect();
        match &rt {
            Ty::Error => {
                self.check_args_loose(args);
                return Ty::Error;
            }
            Ty::Nullable(_) | Ty::NoneT => {
                let shown = self.show(&rt);
                self.err(
                    "T162",
                    format!("this value may be `none` (`{shown}`): check `!= none` before calling `.{}()`", name.name),
                    callee.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
            _ => {}
        }
        // built-in methods first for built-in types
        if !matches!(rt, Ty::Struct(..) | Ty::Enum(_) | Ty::Param(_) | Ty::Trait(..))
            && let Some((params, ret)) = self.builtin_method(&rt, &name.name) {
                self.check_builtin_constraint(e, &rt, &name.name, callee.span);
                let sig = CallSig { name: name.name.clone(), tparams: vec![], params, ret };
                return self.check_args(e.span, &sig, &[], HashMap::new(), args);
            }
        if let Some(um) = self.find_user_method(&rt, &name.name) {
            let f = self.defs.fns[um.fid].clone();
            if !f.has_self {
                let shown = self.show(&rt);
                self.err(
                    "T243",
                    format!("`{0}` is a static method: call it as `{shown}.{0}(...)`, not on a value", name.name),
                    callee.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
            let imp = self.defs.impls[um.impl_id].clone();
            if imp.trait_.is_none()
                && let Ty::Struct(owner, _) = imp.target {
                    self.check_vis(f.vis, owner, name.span, &format!("method `{}`", name.name));
                }
            let own: Vec<ParamId> = f.scope_params.iter().rev().take(f.own_tparams).rev().map(|(_, p)| *p).collect();
            let sig = CallSig { name: name.name.clone(), tparams: own, params: f.params.clone(), ret: f.ret.clone() };
            return self.check_args(e.span, &sig, &explicit, um.map, args);
        }
        // trait methods through a generic bound or a trait-typed value
        let trait_refs: Vec<(TraitId, Vec<Ty>)> = match &rt {
            Ty::Param(p) => self.defs.params[*p].bounds.clone(),
            Ty::Trait(t, a) => vec![(*t, a.clone())],
            _ => vec![],
        };
        for (tid, targs2) in trait_refs {
            let td = self.defs.traits[tid].clone();
            if let Some(sig) = td.sigs.iter().find(|s| s.name == name.name && s.has_self) {
                let mut map: HashMap<ParamId, Ty> = HashMap::new();
                map.insert(SELF_PARAM, rt.clone());
                for (p, a) in td.tparams.iter().zip(targs2.iter()) {
                    map.insert(*p, a.clone());
                }
                let cs = CallSig {
                    name: name.name.clone(),
                    tparams: sig.tparams.clone(),
                    params: sig.params.iter().map(|p| p.subst(&map)).collect(),
                    ret: sig.ret.clone().map(|r| r.subst(&map)).unwrap_or(Ty::Error),
                };
                return self.check_args(e.span, &cs, &explicit, map, args);
            }
        }
        // a field holding a function: `obj.callback(x)`
        if let Ty::Struct(..) = &rt {
            let probe = Ident { name: name.name.clone(), span: name.span };
            let before = self.diags.len();
            let ft = self.field_of(e, &rt, &probe, true);
            if self.diags.len() == before {
                if let Ty::Fn(params, ret) = self.resolve(&ft) {
                    let sig = CallSig { name: name.name.clone(), tparams: vec![], params, ret: *ret };
                    return self.check_args(e.span, &sig, &[], HashMap::new(), args);
                }
            } else {
                self.diags.truncate(before);
            }
        }
        let shown = self.show(&rt);
        self.err("T161", format!("`{shown}` has no method `{}`", name.name), name.span);
        self.check_args_loose(args);
        Ty::Error
    }

    fn call_static(&mut self, e: &Expr, callee: &Expr, ty: &Ty, tref: &TypeRef, name: &Ident, targs: &[TypeExpr], args: &[Expr]) -> Ty {
        let explicit: Vec<Ty> = targs.iter().map(|t| self.resolve_type(t)).collect();
        if let TypeRef::Builtin(b) = tref {
            if name.name == "parse" && (*b == "int" || *b == "float") {
                self.res.paths.insert(callee.id, PathRes::BuiltinStatic(if *b == "int" { "int" } else { "float" }, "parse"));
                let ret = Ty::nullable(if *b == "int" { Ty::Int } else { Ty::Float });
                let sig = CallSig { name: format!("{b}.parse"), tparams: vec![], params: vec![Ty::Str], ret };
                return self.check_args(e.span, &sig, &[], HashMap::new(), args);
            }
            self.err("T161", format!("`{b}` has no static member `{}`", name.name), name.span);
            self.check_args_loose(args);
            return Ty::Error;
        }
        // a static field holding a function, or an enum variant: not callable here
        if let Some(um) = self.find_user_method(ty, &name.name) {
            let f = self.defs.fns[um.fid].clone();
            if f.has_self {
                let shown = self.show(ty);
                self.err(
                    "T244",
                    format!("`{0}` needs a value: call it on an instance (`value.{0}(...)`), not on `{shown}`", name.name),
                    callee.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
            let imp = self.defs.impls[um.impl_id].clone();
            if imp.trait_.is_none()
                && let Ty::Struct(owner, _) = imp.target {
                    self.check_vis(f.vis, owner, name.span, &format!("method `{}`", name.name));
                }
            self.res.paths.insert(callee.id, PathRes::Fn(um.fid));
            let own: Vec<ParamId> = f.scope_params.iter().rev().take(f.own_tparams).rev().map(|(_, p)| *p).collect();
            let sig = CallSig { name: name.name.clone(), tparams: own, params: f.params.clone(), ret: f.ret.clone() };
            return self.check_args(e.span, &sig, &explicit, um.map, args);
        }
        // otherwise a static field/variant: evaluate as a value and call it
        let before = self.diags.len();
        let vt = self.static_member_value(callee, ty, tref, name, targs);
        if let Ty::Fn(params, ret) = self.resolve(&vt) {
            let sig = CallSig { name: name.name.clone(), tparams: vec![], params, ret: *ret };
            return self.check_args(e.span, &sig, &[], HashMap::new(), args);
        }
        if self.diags.len() == before {
            let shown = self.show(&vt);
            self.err("T241", format!("cannot call a `{shown}`: it is not a function"), callee.span);
        }
        self.check_args_loose(args);
        Ty::Error
    }

    fn call_super(&mut self, e: &Expr, callee: &Expr, name: &Ident, args: &[Expr]) -> Ty {
        let fail = |this: &mut Checker, msg: String| {
            this.err("T152", msg, callee.span);
            this.check_args_loose(args);
            Ty::Error
        };
        let Some(fid) = self.cur_fn else {
            return fail(self, "`super` can only be used inside a method".to_string());
        };
        let Some(iid) = self.defs.fns[fid].impl_id else {
            return fail(self, "`super` can only be used inside a method".to_string());
        };
        let target = self.defs.impls[iid].target.clone();
        let Ty::Struct(sid, sargs) = target else {
            return fail(self, "`super` can only be used in methods of a struct".to_string());
        };
        let Some((pid, pargs)) = self.defs.structs[sid].parent.clone() else {
            let n = self.defs.structs[sid].name.clone();
            return fail(self, format!("`super` needs a parent: struct `{n}` does not extend another struct"));
        };
        let map: HashMap<ParamId, Ty> = self.defs.structs[sid].tparams.iter().cloned().zip(sargs.iter().cloned()).collect();
        let pty = Ty::Struct(pid, pargs.iter().map(|t| t.subst(&map)).collect());
        let Some(um) = self.find_user_method(&pty, &name.name) else {
            let pn = self.defs.structs[pid].name.clone();
            return fail(self, format!("the parent struct `{pn}` has no method `{}`", name.name));
        };
        let f = self.defs.fns[um.fid].clone();
        if !f.has_self {
            return fail(self, format!("`{}` is a static method, it cannot be called with `super`", name.name));
        }
        self.res.super_calls.insert(callee.id, um.fid);
        let own: Vec<ParamId> = f.scope_params.iter().rev().take(f.own_tparams).rev().map(|(_, p)| *p).collect();
        let sig = CallSig { name: name.name.clone(), tparams: own, params: f.params.clone(), ret: f.ret.clone() };
        self.check_args(e.span, &sig, &[], um.map, args)
    }

    fn call_builtin_fn(&mut self, e: &Expr, callee: &Expr, name: &str, args: &[Expr]) -> Ty {
        let static_name: &'static str = match name {
            "print" => "print",
            "write" => "write",
            "read" => "read",
            "panic" => "panic",
            _ => "assert",
        };
        self.res.paths.insert(callee.id, PathRes::BuiltinFn(static_name));
        let arity = |this: &mut Checker, min: usize, max: usize| -> bool {
            if args.len() < min || args.len() > max {
                let n = if min == max { format!("{min}") } else { format!("{min} to {max}") };
                this.err("T240", format!("`{name}` expects {n} argument(s), found {}", args.len()), e.span);
                this.check_args_loose(args);
                false
            } else {
                true
            }
        };
        match name {
            "print" | "write" => {
                if !arity(self, if name == "print" { 0 } else { 1 }, 1) {
                    return Ty::Unit;
                }
                for a in args {
                    let t = self.expr(a, &Want::Any);
                    self.require_value(t, a.span);
                }
                Ty::Unit
            }
            "read" => {
                if !self.caps.read {
                    self.err("T250", "`read` is not available in this environment (there is no input to read)", e.span);
                }
                arity(self, 0, 0);
                Ty::Str
            }
            "panic" => {
                if arity(self, 1, 1) {
                    let t = self.expr(&args[0], &Want::Any);
                    self.require_value(t, args[0].span);
                }
                Ty::Never
            }
            _ => {
                if arity(self, 1, 2) {
                    let t = self.expr(&args[0], &Want::Exp(Ty::Bool));
                    if !self.assignable(&t, &Ty::Bool) {
                        self.mismatch(args[0].span, &Ty::Bool, &t);
                    }
                    if let Some(m) = args.get(1) {
                        let t = self.expr(m, &Want::Any);
                        self.require_value(t, m.span);
                    }
                }
                Ty::Unit
            }
        }
    }

    /// Built-in array methods that need something of the element type.
    fn check_builtin_constraint(&mut self, e: &Expr, recv: &Ty, name: &str, span: Span) {
        let Ty::Array(t) = recv else { return };
        let t = self.resolve(t);
        if matches!(t, Ty::Never | Ty::Error) {
            return;
        }
        match name {
            "sort" | "min" | "max" => {
                if self.find_lang_impl(&t, Lang::Ord, std::slice::from_ref(&t)).is_none() {
                    let shown = self.show(&t);
                    self.err(
                        "T260",
                        format!("`{name}()` needs elements that can be ordered, but `{shown}` cannot (implement `Ord` for it)"),
                        span,
                    );
                }
            }
            "sum" => match t {
                Ty::Int => {}
                Ty::Float => {
                    self.res.float_sums.insert(e.id);
                }
                _ => {
                    let shown = self.show(&t);
                    self.err("T260", format!("`sum()` needs an array of numbers, not `array<{shown}>`"), span);
                }
            },
            _ => {}
        }
    }

    /// Signatures of the methods the language provides on its own types.
    pub fn builtin_method(&mut self, recv: &Ty, name: &str) -> Option<(Vec<Ty>, Ty)> {
        let s = Ty::Str;
        let arr = |t: &Ty| Ty::Array(Box::new(t.clone()));
        Some(match (recv, name) {
            (Ty::Array(_), "len") => (vec![], Ty::Int),
            (Ty::Array(_), "is_empty") => (vec![], Ty::Bool),
            (Ty::Array(t), "push") => (vec![(**t).clone()], Ty::Unit),
            (Ty::Array(t), "pop") => (vec![], Ty::nullable((**t).clone())),
            (Ty::Array(t), "insert") => (vec![Ty::Int, (**t).clone()], Ty::Unit),
            (Ty::Array(t), "remove") => (vec![Ty::Int], (**t).clone()),
            (Ty::Array(_), "clear") => (vec![], Ty::Unit),
            (Ty::Array(t), "contains") => (vec![(**t).clone()], Ty::Bool),
            (Ty::Array(t), "index_of") => (vec![(**t).clone()], Ty::nullable(Ty::Int)),
            (Ty::Array(_), "reverse") => (vec![], Ty::Unit),
            (Ty::Array(_), "join") => (vec![s.clone()], s),
            (Ty::Array(_), "sort") => (vec![], Ty::Unit),
            (Ty::Array(t), "min" | "max") => (vec![], Ty::nullable((**t).clone())),
            (Ty::Array(t), "sum") => (vec![], (**t).clone()),
            (Ty::Array(t), "slice") => (vec![Ty::Int, Ty::Int], arr(t)),
            (Ty::Str, "len") => (vec![], Ty::Int),
            (Ty::Str, "is_empty") => (vec![], Ty::Bool),
            (Ty::Str, "upper" | "lower" | "trim") => (vec![], Ty::Str),
            (Ty::Str, "contains" | "starts_with" | "ends_with") => (vec![Ty::Str], Ty::Bool),
            (Ty::Str, "index_of") => (vec![Ty::Str], Ty::nullable(Ty::Int)),
            (Ty::Str, "replace") => (vec![Ty::Str, Ty::Str], Ty::Str),
            (Ty::Str, "split") => (vec![Ty::Str], arr(&Ty::Str)),
            (Ty::Str, "repeat") => (vec![Ty::Int], Ty::Str),
            (Ty::Str, "chars") => (vec![], arr(&Ty::Str)),
            (Ty::Str, "substring") => (vec![Ty::Int, Ty::Int], Ty::Str),
            (Ty::Int, "abs") => (vec![], Ty::Int),
            (Ty::Int, "min" | "max" | "pow") => (vec![Ty::Int], Ty::Int),
            (Ty::Int, "sqrt") => (vec![], Ty::Float),
            (Ty::Float, "abs" | "sqrt") => (vec![], Ty::Float),
            (Ty::Float, "min" | "max" | "pow") => (vec![Ty::Float], Ty::Float),
            (Ty::Float, "floor" | "ceil" | "round") => (vec![], Ty::Int),
            _ => return None,
        })
    }
}

fn callee_name(callee: &Expr) -> String {
    match &callee.kind {
        ExprKind::Ident(n, _) => n.clone(),
        ExprKind::Field(_, n, _) => n.name.clone(),
        _ => "function".to_string(),
    }
}
