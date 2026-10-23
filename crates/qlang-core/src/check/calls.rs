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
    /// The parameters' own names, in order, when known: lets a call use
    /// named arguments (`f(a: 1)`). `None` for calls where names aren't
    /// available (built-ins, trait dispatch through a bound), where named
    /// arguments are rejected.
    pub param_names: Option<Vec<String>>,
}

/// The parameter names of a declared function, for named-argument resolution.
fn fn_param_names(decl: &Option<Rc<FunDecl>>) -> Option<Vec<String>> {
    decl.as_ref()
        .map(|d| d.params.iter().map(|p| p.name.name.clone()).collect())
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
        let head = self.try_head_inner(e);
        if self.collecting() {
            match &head {
                Some(Head::Type(_, tref)) => {
                    let key = match tref {
                        TypeRef::Struct(id) => self.defs.structs[*id].name.clone(),
                        TypeRef::Enum(id) => self.defs.enums[*id].name.clone(),
                        TypeRef::Builtin(b) => b.to_string(),
                    };
                    if let Some(c) = &mut self.collect {
                        c.head(
                            e,
                            Ty::Unit,
                            format!("type {key}"),
                            super::info::StaticKey::Type(tref.clone()),
                        );
                    }
                }
                Some(Head::Module(m)) => {
                    if let Some(c) = &mut self.collect {
                        c.head(
                            e,
                            Ty::Unit,
                            format!("module {m}"),
                            super::info::StaticKey::Module(*m),
                        );
                    }
                }
                _ => {}
            }
        }
        head
    }

    fn try_head_inner(&mut self, e: &Expr) -> Option<Head> {
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
                    Some(item @ (ItemRef::Struct(_) | ItemRef::Enum(_))) => {
                        Some(self.type_head(e, item, targs))
                    }
                    Some(_) => None,
                    None => {
                        let (ty, b) = match name.as_str() {
                            "int" => (Ty::Int, "int"),
                            "float" => (Ty::Float, "float"),
                            "bool" => (Ty::Bool, "bool"),
                            "string" => (Ty::Str, "string"),
                            "uuid" => (Ty::Uuid, "uuid"),
                            "ulid" => (Ty::Ulid, "ulid"),
                            "datetime" => (Ty::DateTime, "datetime"),
                            // namespaces: they only hold functions
                            "time" => (Ty::Unit, "time"),
                            "random" => (Ty::Unit, "random"),
                            "regex" => (Ty::Unit, "regex"),
                            "Base64" => (Ty::Unit, "Base64"),
                            "Hex" => (Ty::Unit, "Hex"),
                            "Url" => (Ty::Unit, "Url"),
                            "Hash" => (Ty::Unit, "Hash"),
                            "array" => {
                                let f = self.fresh();
                                (Ty::Array(Box::new(f)), "array")
                            }
                            _ => return None,
                        };
                        self.res
                            .paths
                            .insert(e.id, PathRes::Type(TypeRef::Builtin(b)));
                        Some(Head::Type(ty, TypeRef::Builtin(b)))
                    }
                }
            }
            ExprKind::Field(obj, name, targs) => match self.try_head(obj)? {
                Head::Module(m) => {
                    let Some(item) = self.module_member(m, name) else {
                        return Some(Head::Error);
                    };
                    match item {
                        ItemRef::Module(m2) => {
                            self.res.paths.insert(e.id, PathRes::Module(m2));
                            Some(Head::Module(m2))
                        }
                        ItemRef::Struct(_) | ItemRef::Enum(_) => {
                            Some(self.type_head(e, item, targs))
                        }
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
                    self.err(
                        "T121",
                        format!(
                            "`{}` expects {n} type argument(s)",
                            self.defs.structs[id].name
                        ),
                        e.span,
                    );
                    return Head::Error;
                };
                self.res
                    .paths
                    .insert(e.id, PathRes::Type(TypeRef::Struct(id)));
                Head::Type(Ty::Struct(id, args), TypeRef::Struct(id))
            }
            ItemRef::Enum(id) => {
                let n = self.defs.enums[id].tparams.len();
                let args: Vec<Ty> = if targs.is_empty() {
                    (0..n).map(|_| self.fresh()).collect()
                } else if targs.len() == n {
                    targs.iter().map(|t| self.resolve_type(t)).collect()
                } else {
                    self.err(
                        "T121",
                        format!(
                            "`{}` expects {n} type argument(s)",
                            self.defs.enums[id].name
                        ),
                        e.span,
                    );
                    return Head::Error;
                };
                self.res
                    .paths
                    .insert(e.id, PathRes::Type(TypeRef::Enum(id)));
                Head::Type(Ty::Enum(id, args), TypeRef::Enum(id))
            }
            _ => Head::Error,
        }
    }

    // ------------------------------------------------------------ field access

    pub fn check_field(&mut self, e: &Expr, obj: &Expr, name: &Ident, targs: &[TypeExpr]) -> Ty {
        match self.try_head(obj) {
            Some(Head::Error) => Ty::Error,
            Some(Head::Module(m)) => {
                let Some(item) = self.module_member(m, name) else {
                    return Ty::Error;
                };
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
                        self.err(
                            "T154",
                            format!("`{}` is a type or a module, not a value", name.name),
                            e.span,
                        );
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

    pub fn check_field_target(
        &mut self,
        target: &Expr,
        obj: &Expr,
        name: &Ident,
        targs: &[TypeExpr],
    ) -> Ty {
        let t = self.check_field(target, obj, name, targs);
        if matches!(
            self.res.paths.get(&target.id),
            Some(PathRes::EnumVariant(..) | PathRes::Fn(_) | PathRes::Module(_))
        ) {
            self.err(
                "T212",
                format!("cannot assign to `{}`", name.name),
                target.span,
            );
        }
        t
    }

    fn static_member_value(
        &mut self,
        e: &Expr,
        ty: &Ty,
        tref: &TypeRef,
        name: &Ident,
        targs: &[TypeExpr],
    ) -> Ty {
        match tref {
            TypeRef::Enum(eid) => {
                if let Some(vi) = self.defs.enums[*eid].variant_index(&name.name) {
                    self.res.paths.insert(e.id, PathRes::EnumVariant(*eid, vi));
                    if !self.defs.enums[*eid].variants[vi].fields.is_empty() {
                        self.err(
                            "T164",
                            format!(
                                "`{}.{}` carries data: write `{}.{}(...)` with its values",
                                self.defs.enums[*eid].name,
                                name.name,
                                self.defs.enums[*eid].name,
                                name.name
                            ),
                            e.span,
                        );
                        return Ty::Error;
                    }
                    self.res.expr_tys.insert(e.id, ty.clone());
                    return ty.clone();
                }
            }
            TypeRef::Struct(sid) => {
                let Ty::Struct(_, args) = ty else {
                    return Ty::Error;
                };
                for (asid, aargs) in self.defs.ancestry(*sid, args) {
                    let ad = self.defs.structs[asid].clone();
                    if let Some(f) = ad
                        .fields
                        .iter()
                        .find(|f| f.is_static && f.name == name.name)
                    {
                        let map: HashMap<ParamId, Ty> = ad
                            .tparams
                            .iter()
                            .cloned()
                            .zip(aargs.iter().cloned())
                            .collect();
                        self.check_vis(
                            f.vis,
                            asid,
                            name.span,
                            &format!("static field `{}`", name.name),
                        );
                        self.res
                            .paths
                            .insert(e.id, PathRes::StaticField(asid, name.name.clone()));
                        return f.ty.subst(&map);
                    }
                }
            }
            TypeRef::Builtin(b) => {
                let found = match (*b, name.name.as_str()) {
                    ("float", "PI") => Some(("float", "PI", Ty::Float)),
                    ("float", "E") => Some(("float", "E", Ty::Float)),
                    ("float", "INFINITY") => Some(("float", "INFINITY", Ty::Float)),
                    ("float", "NAN") => Some(("float", "NAN", Ty::Float)),
                    ("int", "MAX") => Some(("int", "MAX", Ty::Int)),
                    ("int", "MIN") => Some(("int", "MIN", Ty::Int)),
                    ("uuid", "NIL") => Some(("uuid", "NIL", Ty::Uuid)),
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
                return Ty::Fn(
                    f.params.iter().map(|p| p.subst(&um.map)).collect(),
                    Box::new(f.ret.subst(&um.map)),
                );
            }
        }
        if name.name == "class" {
            let class_name = match tref {
                TypeRef::Struct(sid) => Some(self.defs.structs[*sid].name.clone()),
                TypeRef::Enum(eid) => Some(self.defs.enums[*eid].name.clone()),
                _ => None,
            };
            if let Some(n) = class_name {
                self.res.paths.insert(e.id, PathRes::ClassName(n));
                return Ty::Str;
            }
        }
        let shown = self.show(ty);
        self.err(
            "T161",
            format!("`{shown}` has no static member `{}`", name.name),
            name.span,
        );
        Ty::Error
    }

    pub fn field_of(&mut self, e: &Expr, ot: &Ty, name: &Ident, obj_is_variable: bool) -> Ty {
        let ot = self.resolve(ot);
        match &ot {
            Ty::Error => Ty::Error,
            Ty::Nullable(_) | Ty::NoneT => {
                let shown = self.show(&ot);
                let hint = if obj_is_variable {
                    ""
                } else {
                    " (a field or a call cannot be checked in place: copy it into a variable first, then test the variable)"
                };
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
                                format!(
                                    "`{}` is a static field: use `{}.{}`",
                                    name.name, ad.name, name.name
                                ),
                                name.span,
                            );
                            return Ty::Error;
                        }
                        let map: HashMap<ParamId, Ty> = ad
                            .tparams
                            .iter()
                            .cloned()
                            .zip(aargs.iter().cloned())
                            .collect();
                        self.check_vis(f.vis, asid, name.span, &format!("field `{}`", name.name));
                        return f.ty.subst(&map);
                    }
                }
                let is_method = self.find_user_method(&ot, &name.name).is_some();
                let shown = self.show(&ot);
                if is_method {
                    self.err(
                        "T164",
                        format!(
                            "`{}` is a method of `{shown}`: call it with `()`",
                            name.name
                        ),
                        name.span,
                    );
                } else {
                    self.err(
                        "T161",
                        format!("`{shown}` has no field `{}`", name.name),
                        name.span,
                    );
                }
                Ty::Error
            }
            other => {
                let shown = other.show(&self.defs);
                if self.builtin_method(other, &name.name).is_some() {
                    self.err(
                        "T164",
                        format!(
                            "`{}` is a method of `{shown}`: call it with `()`",
                            name.name
                        ),
                        name.span,
                    );
                } else {
                    self.err(
                        "T161",
                        format!("`{shown}` has no field `{}`", name.name),
                        name.span,
                    );
                }
                Ty::Error
            }
        }
    }

    // ------------------------------------------------------------------ calls

    /// Check arguments loosely after an error, to still report problems inside them.
    fn check_args_loose(&mut self, args: &[CallArg]) {
        for a in args {
            self.expr(&a.value, &Want::Any);
        }
    }

    pub fn check_call(&mut self, e: &Expr, callee: &Expr, args: &[CallArg], want: &Want) -> Ty {
        match &callee.kind {
            ExprKind::Field(obj, name, targs) => {
                if matches!(obj.kind, ExprKind::Super) {
                    return self.call_super(e, callee, name, args, want);
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
                            ItemRef::Fn(fid) => {
                                return self.call_fn_item(e, callee, fid, targs, args, want);
                            }
                            ItemRef::Struct(_) => {
                                self.err(
                                    "T154",
                                    format!(
                                        "`{0}` is a type: use `{0}.new(...)` or `{0} {{ ... }}`",
                                        name.name
                                    ),
                                    callee.span,
                                );
                                self.check_args_loose(args);
                                return Ty::Error;
                            }
                            _ => {}
                        }
                        // a variable holding a function: fall through to a value call
                    }
                    Some(Head::Type(ty, tref)) => {
                        if let Ty::Struct(sid, _) = &ty
                            && self.defs.structs[*sid].module == PRELUDE_MOD
                            && self.defs.structs[*sid].name == "Reflect"
                        {
                            return self.call_reflect(callee, name, targs, args);
                        }
                        return self.call_static(e, callee, &ty, &tref, name, targs, args, want);
                    }
                    None => {
                        let rt = self.expr(obj, &Want::Any);
                        return self.call_method(e, callee, &rt, name, targs, args, want);
                    }
                }
            }
            ExprKind::Ident(name, targs) if self.lookup_var(name).is_none() => {
                match self.lookup_item(name) {
                    Some(ItemRef::Fn(fid)) => {
                        return self.call_fn_item(e, callee, fid, targs, args, want);
                    }
                    Some(ItemRef::Struct(_) | ItemRef::Enum(_)) => {
                        self.err(
                                "T154",
                                format!("`{name}` is a type, not a function: use `{name}.new(...)` or `{name} {{ ... }}`"),
                                callee.span,
                            );
                        self.check_args_loose(args);
                        return Ty::Error;
                    }
                    None if decls::BUILTIN_FNS.contains(&name.as_str()) => {
                        return self.call_builtin_fn(e, callee, name, args);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // call a value: a variable or an expression of function type
        let ct = self.expr(callee, &Want::Any);
        match self.resolve(&ct) {
            Ty::Fn(params, ret) => {
                let sig = CallSig {
                    name: callee_name(callee),
                    tparams: vec![],
                    params,
                    ret: *ret,
                    param_names: None,
                };
                self.check_args(e, &sig, &[], HashMap::new(), args, want)
            }
            Ty::Error => {
                self.check_args_loose(args);
                Ty::Error
            }
            other => {
                let shown = other.show(&self.defs);
                self.err(
                    "T241",
                    format!("cannot call a `{shown}`: it is not a function"),
                    callee.span,
                );
                self.check_args_loose(args);
                Ty::Error
            }
        }
    }

    /// The prelude's `Db`, `fs`, `env` and `http` need something from the host. Their
    /// `extern fun` are always declared, so it is the call on them that says so.
    fn require_extern(&mut self, ext: &str, what: &str, why: &str, span: Span) {
        if self.missing_externs.contains(ext) {
            self.err(
                "T250",
                format!("`{what}` is not available in this environment ({why})"),
                span,
            );
        }
    }

    /// Using an `extern fun` the host does not provide is a compile error.
    pub(crate) fn check_extern_available(&mut self, fid: FnId, span: Span) {
        let def = &self.defs.fns[fid];
        // the prelude's own externs are reported where they are used (`Db` calls)
        if self.cur_mod != PRELUDE_MOD
            && def.decl.as_ref().is_some_and(|d| d.external)
            && self.missing_externs.contains(&def.name)
        {
            let mut msg = format!(
                "`{}` is not available in this environment (the host does not provide it)",
                def.name
            );
            // declared by a package: the project has to provide it
            let module = self.modules[def.module].path.clone();
            if let Some((package, _)) = module.split_once('/').filter(|_| module.starts_with('@')) {
                msg.push_str(&format!(
                    "; the package {package} needs it: provide it with `--extern` or in the `[externs]` of this project's qlang.toml"
                ));
            }
            self.err("T263", msg, span);
        }
    }

    fn call_fn_item(
        &mut self,
        e: &Expr,
        callee: &Expr,
        fid: FnId,
        targs: &[TypeExpr],
        args: &[CallArg],
        want: &Want,
    ) -> Ty {
        let def = self.defs.fns[fid].clone();
        self.check_extern_available(fid, e.span);
        self.res.paths.insert(callee.id, PathRes::Fn(fid));
        if let Some(c) = &mut self.collect {
            c.expr(
                callee,
                Ty::Fn(def.params.clone(), Box::new(def.ret.clone())),
            );
        }
        let own: Vec<ParamId> = def
            .scope_params
            .iter()
            .rev()
            .take(def.own_tparams)
            .rev()
            .map(|(_, p)| *p)
            .collect();
        let explicit: Vec<Ty> = targs.iter().map(|t| self.resolve_type(t)).collect();
        let sig = CallSig {
            name: def.name.clone(),
            tparams: own,
            params: def.params.clone(),
            ret: def.ret.clone(),
            param_names: fn_param_names(&def.decl),
        };
        let generic = !sig.tparams.is_empty();
        let out = self.check_args(e, &sig, &explicit, HashMap::new(), args, want);
        if generic {
            self.res.expr_tys.insert(e.id, out.clone());
        }
        out
    }

    /// Resolve `args` (possibly mixing positional and named arguments) into one value
    /// expression per parameter, in parameter order. Reports and returns `None` on any
    /// arity/name mismatch, after still checking every argument loosely for other errors.
    fn order_args<'a>(
        &mut self,
        e: &Expr,
        sig: &CallSig,
        params: &[Ty],
        args: &'a [CallArg],
    ) -> Option<Vec<&'a Expr>> {
        if args.iter().all(|a| a.name.is_none()) {
            if args.len() != params.len() {
                self.err(
                    "T240",
                    format!(
                        "`{}` expects {} argument(s), found {}",
                        sig.name,
                        params.len(),
                        args.len()
                    ),
                    e.span,
                );
                self.check_args_loose(args);
                return None;
            }
            return Some(args.iter().map(|a| &a.value).collect());
        }
        let Some(names) = &sig.param_names else {
            self.err(
                "T245",
                format!("`{}` does not accept named arguments", sig.name),
                e.span,
            );
            self.check_args_loose(args);
            return None;
        };
        let mut slots: Vec<Option<&Expr>> = vec![None; params.len()];
        // slot filled by each argument, in writing order: lets the interpreter
        // evaluate arguments in writing order, then place each value correctly.
        let mut order: Vec<usize> = Vec::with_capacity(args.len());
        let mut seen_named = false;
        let mut ok = true;
        for (i, a) in args.iter().enumerate() {
            match &a.name {
                None => {
                    if seen_named {
                        self.err(
                            "T246",
                            "positional arguments must come before named arguments",
                            a.value.span,
                        );
                        ok = false;
                    } else if i < slots.len() {
                        slots[i] = Some(&a.value);
                        order.push(i);
                    } else {
                        ok = false;
                    }
                }
                Some(n) => {
                    seen_named = true;
                    let Some(idx) = names.iter().position(|p| p == &n.name) else {
                        self.err(
                            "T247",
                            format!("`{}` has no parameter named `{}`", sig.name, n.name),
                            n.span,
                        );
                        ok = false;
                        continue;
                    };
                    if slots[idx].is_some() {
                        self.err(
                            "T248",
                            format!("parameter `{}` is given more than once", n.name),
                            n.span,
                        );
                        ok = false;
                        continue;
                    }
                    slots[idx] = Some(&a.value);
                    order.push(idx);
                }
            }
        }
        if !ok {
            self.check_args_loose(args);
            return None;
        }
        if let Some(missing) = slots.iter().position(|s| s.is_none()) {
            let pn = names.get(missing).map(|s| s.as_str()).unwrap_or("?");
            self.err(
                "T249",
                format!("`{}` is missing the argument for `{pn}`", sig.name),
                e.span,
            );
            self.check_args_loose(args);
            return None;
        }
        self.res.arg_order.insert(e.id, order);
        Some(slots.into_iter().map(|s| s.unwrap()).collect())
    }

    /// Check arguments against a signature, inferring generic parameters. `want` is the
    /// type expected of the call's result: when an argument leaves some type parameters
    /// unresolved, matching the result against `want` can resolve the rest (`let x: Box<int>
    /// = Box.empty()`, `return Result.ok(1)` inside a function returning `Result<int, E>`...).
    pub fn check_args(
        &mut self,
        e: &Expr,
        sig: &CallSig,
        explicit: &[Ty],
        mut map: HashMap<ParamId, Ty>,
        args: &[CallArg],
        want: &Want,
    ) -> Ty {
        if !explicit.is_empty() {
            if explicit.len() != sig.tparams.len() {
                self.err(
                    "T121",
                    format!(
                        "`{}` expects {} type argument(s), found {}",
                        sig.name,
                        sig.tparams.len(),
                        explicit.len()
                    ),
                    e.span,
                );
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
        let Some(ordered) = self.order_args(e, sig, &params, args) else {
            return self.resolve(&ret);
        };
        for (a, p) in ordered.iter().zip(params.iter()) {
            let want = self.resolve(p);
            let t = self.expr(a, &Want::Exp(want.clone()));
            let t = self.require_value(t, a.span);
            if !self.assignable(&t, p) {
                let want = self.resolve(p);
                self.mismatch(a.span, &want, &t);
            }
        }
        for p in &sig.tparams {
            let Some(arg) = map.get(p).cloned() else {
                continue;
            };
            let arg = self.resolve(&arg);
            if arg.contains_infer() {
                continue;
            }
            for (tid, targs) in self.defs.params[*p].bounds.clone() {
                let targs: Vec<Ty> = targs.iter().map(|t| self.resolve(&t.subst(&map))).collect();
                if !self.implements(&arg, tid, &targs) {
                    let (a, tn) = (self.show(&arg), self.show(&Ty::Trait(tid, targs)));
                    let pn = self.defs.params[*p].name.clone();
                    self.err(
                        "T242",
                        format!(
                            "`{a}` cannot be used for `{pn}` in `{}`: it does not implement `{tn}`",
                            sig.name
                        ),
                        e.span,
                    );
                }
            }
        }
        let mut ret = self.resolve(&ret);
        if ret.contains_infer()
            && let Some(expected) = want.expected()
        {
            self.same(&ret, expected);
            ret = self.resolve(&ret);
        }
        if ret.contains_infer() {
            self.err(
                "T171",
                format!("cannot infer the type parameters of `{0}`: write them explicitly, like `{0}<int>(...)`", sig.name),
                e.span,
            );
            return Ty::Error;
        }
        ret
    }

    #[allow(clippy::too_many_arguments)]
    fn call_method(
        &mut self,
        e: &Expr,
        callee: &Expr,
        rt: &Ty,
        name: &Ident,
        targs: &[TypeExpr],
        args: &[CallArg],
        want: &Want,
    ) -> Ty {
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
        // `Db` is written in qlang over `extern fun`: say so up front when the host has no database
        if let Ty::Struct(sid, _) = &rt
            && *sid == self.prelude_struct("Db")
        {
            let needs = match name.name.as_str() {
                "query" => Some("sql_query"),
                "execute" | "begin" | "commit" | "rollback" => Some("sql_execute"),
                "close" => Some("sql_close"),
                _ => None,
            };
            if let Some(ext) = needs {
                self.require_extern(
                    ext,
                    &format!("Db.{}", name.name),
                    "there is no database access",
                    e.span,
                );
            }
        }
        // built-in methods first for built-in types
        if !matches!(
            rt,
            Ty::Struct(..) | Ty::Enum(..) | Ty::Param(_) | Ty::Trait(..)
        ) && let Some((params, ret)) = self.builtin_method(&rt, &name.name)
        {
            self.check_builtin_constraint(e, &rt, &name.name, callee.span);
            let sig = CallSig {
                name: name.name.clone(),
                tparams: vec![],
                params,
                ret,
                param_names: None,
            };
            let out = self.check_args(e, &sig, &[], HashMap::new(), args, want);
            // the typed machine runs the built-in methods whose types it can hold
            let params: Vec<Ty> = sig.params.iter().map(|p| self.resolve(p)).collect();
            let ret = self.resolve(&sig.ret);
            let recv = self.resolve(&rt);
            if super::expr::register_ty(&recv)
                && params.iter().all(super::expr::register_ty)
                && (ret == Ty::Unit || super::expr::register_ty(&ret))
            {
                self.res.builtin_sigs.insert(e.id, (recv, params, ret));
            }
            return out;
        }
        if let Some(um) = self.find_user_method(&rt, &name.name) {
            let f = self.defs.fns[um.fid].clone();
            if !f.has_self {
                let shown = self.show(&rt);
                self.err(
                    "T243",
                    format!(
                        "`{0}` is a static method: call it as `{shown}.{0}(...)`, not on a value",
                        name.name
                    ),
                    callee.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
            let imp = self.defs.impls[um.impl_id].clone();
            if imp.trait_.is_none()
                && let Ty::Struct(owner, _) = imp.target
            {
                self.check_vis(f.vis, owner, name.span, &format!("method `{}`", name.name));
            }
            let own: Vec<ParamId> = f
                .scope_params
                .iter()
                .rev()
                .take(f.own_tparams)
                .rev()
                .map(|(_, p)| *p)
                .collect();
            let sig = CallSig {
                name: name.name.clone(),
                tparams: own,
                params: f.params.clone(),
                ret: f.ret.clone(),
                param_names: fn_param_names(&f.decl),
            };
            return self.check_args(e, &sig, &explicit, um.map, args, want);
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
                self.res.trait_calls.insert(e.id);
                let cs = CallSig {
                    name: name.name.clone(),
                    tparams: sig.tparams.clone(),
                    params: sig.params.iter().map(|p| p.subst(&map)).collect(),
                    ret: sig.ret.clone().map(|r| r.subst(&map)).unwrap_or(Ty::Error),
                    param_names: None,
                };
                return self.check_args(e, &cs, &explicit, map, args, want);
            }
        }
        // a field holding a function: `obj.callback(x)`
        if let Ty::Struct(..) = &rt {
            let probe = Ident {
                name: name.name.clone(),
                span: name.span,
            };
            let before = self.diags.len();
            let ft = self.field_of(e, &rt, &probe, true);
            if self.diags.len() == before {
                if let Ty::Fn(params, ret) = self.resolve(&ft) {
                    let sig = CallSig {
                        name: name.name.clone(),
                        tparams: vec![],
                        params,
                        ret: *ret,
                        param_names: None,
                    };
                    return self.check_args(e, &sig, &[], HashMap::new(), args, want);
                }
            } else {
                self.diags.truncate(before);
            }
        }
        // `value.class()`: the concrete type's name, even through a trait or a generic bound
        if name.name == "class"
            && matches!(
                rt,
                Ty::Struct(..) | Ty::Enum(..) | Ty::Param(_) | Ty::Trait(..)
            )
        {
            self.res.class_calls.insert(e.id);
            let sig = CallSig {
                name: name.name.clone(),
                tparams: vec![],
                params: vec![],
                ret: Ty::Str,
                param_names: None,
            };
            return self.check_args(e, &sig, &[], HashMap::new(), args, want);
        }
        let shown = self.show(&rt);
        self.err(
            "T161",
            format!("`{shown}` has no method `{}`", name.name),
            name.span,
        );
        self.check_args_loose(args);
        Ty::Error
    }

    /// The id of a struct declared in the prelude, by name (`Request`, `Response`...).
    fn prelude_struct(&self, name: &str) -> StructId {
        self.defs
            .structs
            .iter()
            .position(|s| s.module == super::PRELUDE_MOD && s.name == name)
            .unwrap_or_else(|| panic!("prelude struct `{name}` not found"))
    }

    #[allow(clippy::too_many_arguments)]
    fn call_static(
        &mut self,
        e: &Expr,
        callee: &Expr,
        ty: &Ty,
        tref: &TypeRef,
        name: &Ident,
        targs: &[TypeExpr],
        args: &[CallArg],
        want: &Want,
    ) -> Ty {
        let explicit: Vec<Ty> = targs.iter().map(|t| self.resolve_type(t)).collect();
        // `Shape.Circle(2.0)`: building a value of a variant that carries data
        if let (TypeRef::Enum(eid), Ty::Enum(_, targs_of_type)) = (tref, ty)
            && let Some(vi) = self.defs.enums[*eid].variant_index(&name.name)
        {
            let def = self.defs.enums[*eid].clone();
            let variant = &def.variants[vi];
            self.res
                .paths
                .insert(callee.id, PathRes::EnumVariant(*eid, vi));
            if variant.fields.is_empty() {
                self.err(
                    "T164",
                    format!(
                        "`{0}.{1}` carries no data: write `{0}.{1}` without ()",
                        def.name, variant.name
                    ),
                    e.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
            let map: HashMap<ParamId, Ty> = def
                .tparams
                .iter()
                .copied()
                .zip(targs_of_type.iter().cloned())
                .collect();
            let sig = CallSig {
                name: format!("{}.{}", def.name, variant.name),
                tparams: vec![],
                params: variant.fields.iter().map(|(_, t)| t.subst(&map)).collect(),
                ret: ty.clone(),
                param_names: Some(variant.fields.iter().map(|(n, _)| n.clone()).collect()),
            };
            let out = self.check_args(e, &sig, &[], HashMap::new(), args, want);
            self.res.expr_tys.insert(e.id, ty.clone());
            return out;
        }
        // what the host has to provide for the prelude's `fs`, `env`, `http` and `Db`
        if let TypeRef::Struct(sid) = tref {
            let owner = &self.defs.structs[*sid];
            if owner.module == PRELUDE_MOD {
                let need = match (owner.name.as_str(), name.name.as_str()) {
                    ("Db", "connect") => Some(("sql_connect", "there is no database access")),
                    ("fs", "read") => Some(("fs_read", "there is no disk access")),
                    ("fs", "write") => Some(("fs_write", "there is no disk access")),
                    ("env", "get") => Some(("env_get", "there is no process environment")),
                    ("env", "args") => Some(("env_args", "there is no process environment")),
                    ("http", "send") => Some(("http_send", "there is no network access")),
                    ("Process", "run" | "run_with_input") => Some((
                        "process_run",
                        "starting programs is not allowed here; the terminal needs `--allow-process`",
                    )),
                    _ => None,
                };
                if let Some((ext, why)) = need {
                    let what = format!("{}.{}", owner.name, name.name);
                    self.require_extern(ext, &what, why, e.span);
                }
            }
        }
        if let TypeRef::Builtin(b) = tref {
            if let Some((fname, params, ret, needs)) = static_fn(b, &name.name, args.len()) {
                self.res
                    .paths
                    .insert(callee.id, PathRes::BuiltinStatic(b, fname));
                let missing = match needs {
                    Needs::Time if !self.caps.time => Some("there is no clock"),
                    Needs::Random if !self.caps.random => Some("there is no source of randomness"),
                    Needs::TimeAndRandom if !self.caps.time => Some("there is no clock"),
                    Needs::TimeAndRandom if !self.caps.random => {
                        Some("there is no source of randomness")
                    }
                    _ => None,
                };
                if let Some(why) = missing {
                    self.err(
                        "T250",
                        format!("`{b}.{fname}` is not available in this environment ({why})"),
                        e.span,
                    );
                }
                let sig = CallSig {
                    name: format!("{b}.{fname}"),
                    tparams: vec![],
                    params,
                    ret,
                    param_names: None,
                };
                return self.check_args(e, &sig, &[], HashMap::new(), args, want);
            }
            self.err(
                "T161",
                format!("`{b}` has no static member `{}`", name.name),
                name.span,
            );
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
                && let Ty::Struct(owner, _) = imp.target
            {
                self.check_vis(f.vis, owner, name.span, &format!("method `{}`", name.name));
            }
            self.res.paths.insert(callee.id, PathRes::Fn(um.fid));
            let own: Vec<ParamId> = f
                .scope_params
                .iter()
                .rev()
                .take(f.own_tparams)
                .rev()
                .map(|(_, p)| *p)
                .collect();
            let sig = CallSig {
                name: name.name.clone(),
                tparams: own,
                params: f.params.clone(),
                ret: f.ret.clone(),
                param_names: fn_param_names(&f.decl),
            };
            return self.check_args(e, &sig, &explicit, um.map, args, want);
        }
        // otherwise a static field/variant: evaluate as a value and call it
        let before = self.diags.len();
        let vt = self.static_member_value(callee, ty, tref, name, targs);
        if let Ty::Fn(params, ret) = self.resolve(&vt) {
            let sig = CallSig {
                name: name.name.clone(),
                tparams: vec![],
                params,
                ret: *ret,
                param_names: None,
            };
            return self.check_args(e, &sig, &[], HashMap::new(), args, want);
        }
        if self.diags.len() == before {
            let shown = self.show(&vt);
            self.err(
                "T241",
                format!("cannot call a `{shown}`: it is not a function"),
                callee.span,
            );
        }
        self.check_args_loose(args);
        Ty::Error
    }

    fn call_super(
        &mut self,
        e: &Expr,
        callee: &Expr,
        name: &Ident,
        args: &[CallArg],
        want: &Want,
    ) -> Ty {
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
            return fail(
                self,
                "`super` can only be used in methods of a struct".to_string(),
            );
        };
        let Some((pid, pargs)) = self.defs.structs[sid].parent.clone() else {
            let n = self.defs.structs[sid].name.clone();
            return fail(
                self,
                format!("`super` needs a parent: struct `{n}` does not extend another struct"),
            );
        };
        let map: HashMap<ParamId, Ty> = self.defs.structs[sid]
            .tparams
            .iter()
            .cloned()
            .zip(sargs.iter().cloned())
            .collect();
        let pty = Ty::Struct(pid, pargs.iter().map(|t| t.subst(&map)).collect());
        let Some(um) = self.find_user_method(&pty, &name.name) else {
            let pn = self.defs.structs[pid].name.clone();
            return fail(
                self,
                format!("the parent struct `{pn}` has no method `{}`", name.name),
            );
        };
        let f = self.defs.fns[um.fid].clone();
        if !f.has_self {
            return fail(
                self,
                format!(
                    "`{}` is a static method, it cannot be called with `super`",
                    name.name
                ),
            );
        }
        self.res.super_calls.insert(callee.id, um.fid);
        let own: Vec<ParamId> = f
            .scope_params
            .iter()
            .rev()
            .take(f.own_tparams)
            .rev()
            .map(|(_, p)| *p)
            .collect();
        let sig = CallSig {
            name: name.name.clone(),
            tparams: own,
            params: f.params.clone(),
            ret: f.ret.clone(),
            param_names: fn_param_names(&f.decl),
        };
        self.check_args(e, &sig, &[], um.map, args, want)
    }

    fn call_builtin_fn(&mut self, e: &Expr, callee: &Expr, name: &str, args: &[CallArg]) -> Ty {
        let static_name: &'static str = match name {
            "print" => "print",
            "write" => "write",
            "read" => "read",
            "read_all" => "read_all",
            "panic" => "panic",
            "exit" => "exit",
            _ => "assert",
        };
        self.res
            .paths
            .insert(callee.id, PathRes::BuiltinFn(static_name));
        for a in args {
            if let Some(n) = &a.name {
                self.err(
                    "T245",
                    format!("`{name}` does not accept named arguments"),
                    n.span,
                );
            }
        }
        let args: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
        let args = args.as_slice();
        let arity = |this: &mut Checker, min: usize, max: usize| -> bool {
            if args.len() < min || args.len() > max {
                let n = if min == max {
                    format!("{min}")
                } else {
                    format!("{min} to {max}")
                };
                this.err(
                    "T240",
                    format!("`{name}` expects {n} argument(s), found {}", args.len()),
                    e.span,
                );
                for a in args.iter().copied() {
                    this.expr(a, &Want::Any);
                }
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
                for a in args.iter().copied() {
                    let t = self.expr(a, &Want::Any);
                    self.require_value(t, a.span);
                }
                Ty::Unit
            }
            "read" | "read_all" => {
                if !self.caps.read {
                    self.err(
                        "T250",
                        format!("`{name}` is not available in this environment (there is no input to read)"),
                        e.span,
                    );
                }
                arity(self, 0, 0);
                Ty::Str
            }
            "panic" => {
                if arity(self, 1, 1) {
                    let t = self.expr(args[0], &Want::Any);
                    self.require_value(t, args[0].span);
                }
                Ty::Never
            }
            "exit" => {
                if arity(self, 0, 1)
                    && let Some(code) = args.first().copied()
                {
                    let t = self.expr(code, &Want::Exp(Ty::Int));
                    if !self.assignable(&t, &Ty::Int) {
                        self.mismatch(code.span, &Ty::Int, &t);
                    }
                }
                Ty::Never
            }
            _ => {
                if arity(self, 1, 2) {
                    let t = self.expr(args[0], &Want::Exp(Ty::Bool));
                    if !self.assignable(&t, &Ty::Bool) {
                        self.mismatch(args[0].span, &Ty::Bool, &t);
                    }
                    if let Some(m) = args.get(1).copied() {
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
                if self
                    .find_lang_impl(&t, Lang::Ord, std::slice::from_ref(&t))
                    .is_none()
                {
                    let shown = self.show(&t);
                    self.err(
                        "T260",
                        format!("`{name}()` needs elements that can be ordered, but `{shown}` cannot (implement `Ord` for it)"),
                        span,
                    );
                }
            }
            "flatten" => {
                if !matches!(t, Ty::Array(_)) {
                    let shown = self.show(&t);
                    self.err(
                        "T261",
                        format!("`flatten()` needs an array of arrays, not `array<{shown}>`"),
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
                    self.err(
                        "T260",
                        format!("`sum()` needs an array of numbers, not `array<{shown}>`"),
                        span,
                    );
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
            (Ty::Array(t), "map") => {
                let u = self.fresh();
                (
                    vec![Ty::Fn(vec![(**t).clone()], Box::new(u.clone()))],
                    arr(&u),
                )
            }
            (Ty::Array(t), "filter") => (
                vec![Ty::Fn(vec![(**t).clone()], Box::new(Ty::Bool))],
                arr(t),
            ),
            (Ty::Array(t), "reduce") => {
                let u = self.fresh();
                (
                    vec![
                        u.clone(),
                        Ty::Fn(vec![u.clone(), (**t).clone()], Box::new(u.clone())),
                    ],
                    u,
                )
            }
            (Ty::Array(t), "any" | "all") => (
                vec![Ty::Fn(vec![(**t).clone()], Box::new(Ty::Bool))],
                Ty::Bool,
            ),
            (Ty::Array(t), "find") => (
                vec![Ty::Fn(vec![(**t).clone()], Box::new(Ty::Bool))],
                Ty::nullable((**t).clone()),
            ),
            (Ty::Array(t), "each") => (
                vec![Ty::Fn(vec![(**t).clone()], Box::new(Ty::Unit))],
                Ty::Unit,
            ),
            (Ty::Array(t), "sort_by") => (
                vec![Ty::Fn(
                    vec![(**t).clone(), (**t).clone()],
                    Box::new(Ty::Int),
                )],
                Ty::Unit,
            ),
            (Ty::Array(t), "unique") => (vec![], arr(t)),
            (Ty::Array(t), "first" | "last") => (vec![], Ty::nullable((**t).clone())),
            (Ty::Array(t), "concat") => (vec![arr(t)], arr(t)),
            (Ty::Array(t), "take" | "drop") => (vec![Ty::Int], arr(t)),
            (Ty::Array(_), "swap") => (vec![Ty::Int, Ty::Int], Ty::Unit),
            (Ty::Array(t), "chunk") => (vec![Ty::Int], arr(&arr(t))),
            (Ty::Array(t), "repeat") => (vec![Ty::Int], arr(t)),
            (Ty::Array(t), "flatten") => {
                let inner = match self.resolve(t) {
                    Ty::Array(x) => *x,
                    other => other,
                };
                (vec![], arr(&inner))
            }
            (Ty::Map(_, _), "len") => (vec![], Ty::Int),
            (Ty::Map(_, _), "is_empty") => (vec![], Ty::Bool),
            (Ty::Map(_, _), "clear") => (vec![], Ty::Unit),
            (Ty::Map(k, _), "has") => (vec![(**k).clone()], Ty::Bool),
            (Ty::Map(k, v), "get") => (vec![(**k).clone(), (**v).clone()], (**v).clone()),
            (Ty::Map(k, v), "remove") => (vec![(**k).clone()], Ty::nullable((**v).clone())),
            (Ty::Map(k, _), "keys") => (vec![], arr(k)),
            (Ty::Map(_, v), "values") => (vec![], arr(v)),
            (Ty::Map(k, v), "merge") => (
                vec![Ty::Map(k.clone(), v.clone())],
                Ty::Map(k.clone(), v.clone()),
            ),
            (Ty::Map(k, v), "filter") => (
                vec![Ty::Fn(
                    vec![(**k).clone(), (**v).clone()],
                    Box::new(Ty::Bool),
                )],
                Ty::Map(k.clone(), v.clone()),
            ),
            (Ty::Map(k, v), "map_values") => {
                let u = self.fresh();
                (
                    vec![Ty::Fn(
                        vec![(**k).clone(), (**v).clone()],
                        Box::new(u.clone()),
                    )],
                    Ty::Map(k.clone(), Box::new(u)),
                )
            }
            (Ty::Str, "len") => (vec![], Ty::Int),
            (Ty::Str, "is_empty") => (vec![], Ty::Bool),
            (
                Ty::Str,
                "upper" | "lower" | "trim" | "trim_start" | "trim_end" | "capitalize" | "reverse",
            ) => (vec![], Ty::Str),
            (Ty::Str, "snake_case" | "camel_case" | "kebab_case" | "title_case") => {
                (vec![], Ty::Str)
            }
            (Ty::Str, "strip_prefix" | "strip_suffix") => (vec![Ty::Str], Ty::nullable(Ty::Str)),
            (Ty::Str, "eq_ignore_case") => (vec![Ty::Str], Ty::Bool),
            (Ty::Str, "last_index_of") => (vec![Ty::Str], Ty::nullable(Ty::Int)),
            (Ty::Str, "count") => (vec![Ty::Str], Ty::Int),
            (Ty::Str, "lines" | "words") => (vec![], arr(&Ty::Str)),
            (Ty::Str, "pad_left" | "pad_right") => (vec![Ty::Int, Ty::Str], Ty::Str),
            (Ty::Str, "is_digit" | "is_alpha" | "is_blank") => (vec![], Ty::Bool),
            (Ty::Str, "code") => (vec![], Ty::Int),
            (Ty::Str, "to_int") => (vec![], Ty::nullable(Ty::Int)),
            (Ty::Str, "to_float") => (vec![], Ty::nullable(Ty::Float)),
            (Ty::Int, "char") => (vec![], Ty::Str),
            (Ty::Str, "contains" | "starts_with" | "ends_with") => (vec![Ty::Str], Ty::Bool),
            (Ty::Str, "index_of") => (vec![Ty::Str], Ty::nullable(Ty::Int)),
            (Ty::Str, "replace") => (vec![Ty::Str, Ty::Str], Ty::Str),
            (Ty::Str, "split") => (vec![Ty::Str], arr(&Ty::Str)),
            (Ty::Str, "repeat") => (vec![Ty::Int], Ty::Str),
            (Ty::Str, "chars") => (vec![], arr(&Ty::Str)),
            (Ty::Str, "substring") => (vec![Ty::Int, Ty::Int], Ty::Str),
            (Ty::Uuid, "version") => (vec![], Ty::Int),
            (Ty::Ulid, "time") => (vec![], Ty::Int),
            (
                Ty::DateTime,
                "year" | "month" | "day" | "hour" | "minute" | "second" | "weekday" | "ms",
            ) => (vec![], Ty::Int),
            (
                Ty::DateTime,
                "add_years" | "add_months" | "add_days" | "add_hours" | "add_minutes"
                | "add_seconds",
            ) => (vec![Ty::Int], Ty::DateTime),
            (Ty::DateTime, "start_of_day") => (vec![], Ty::DateTime),
            (Ty::DateTime, "days_until") => (vec![Ty::DateTime], Ty::Int),
            (Ty::DateTime, "format") => (vec![Ty::Str], Ty::Str),
            (Ty::Int, "abs") => (vec![], Ty::Int),
            (Ty::Int, "min" | "max" | "pow") => (vec![Ty::Int], Ty::Int),
            (Ty::Int, "sqrt") => (vec![], Ty::Float),
            (Ty::Int, "to_hex" | "to_oct" | "to_bin") => (vec![], Ty::Str),
            (Ty::Float, "abs" | "sqrt") => (vec![], Ty::Float),
            (Ty::Float, "min" | "max" | "pow") => (vec![Ty::Float], Ty::Float),
            (Ty::Float, "floor" | "ceil" | "round") => (vec![], Ty::Int),
            (Ty::Float, "to_fixed") => (vec![Ty::Int], Ty::Str),
            (
                Ty::Int | Ty::Float,
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "ln" | "log10" | "log2"
                | "to_radians" | "to_degrees",
            ) => (vec![], Ty::Float),
            (Ty::Int, "log" | "atan2") => (vec![Ty::Int], Ty::Float),
            (Ty::Float, "log" | "atan2") => (vec![Ty::Float], Ty::Float),
            (Ty::Int, "signum") => (vec![], Ty::Int),
            (Ty::Float, "signum") => (vec![], Ty::Float),
            (Ty::Int, "clamp") => (vec![Ty::Int, Ty::Int], Ty::Int),
            (Ty::Float, "clamp") => (vec![Ty::Float, Ty::Float], Ty::Float),
            (Ty::Float, "trunc") => (vec![], Ty::Int),
            (Ty::Float, "is_nan" | "is_infinite") => (vec![], Ty::Bool),
            _ => return None,
        })
    }
}

/// What a built-in static function needs from the environment.
#[derive(Clone, Copy)]
enum Needs {
    Nothing,
    Time,
    Random,
    TimeAndRandom,
}

/// The static functions of the built-in types: (name, parameters, result, needs).
fn static_fn(owner: &str, name: &str, nargs: usize) -> Option<(&'static str, Vec<Ty>, Ty, Needs)> {
    Some(match (owner, name) {
        ("int", "parse") => (
            "parse",
            vec![Ty::Str],
            Ty::nullable(Ty::Int),
            Needs::Nothing,
        ),
        ("float", "parse") => (
            "parse",
            vec![Ty::Str],
            Ty::nullable(Ty::Float),
            Needs::Nothing,
        ),
        ("uuid", "parse") => (
            "parse",
            vec![Ty::Str],
            Ty::nullable(Ty::Uuid),
            Needs::Nothing,
        ),
        ("uuid", "v4") => ("v4", vec![], Ty::Uuid, Needs::Random),
        ("uuid", "v7") => ("v7", vec![], Ty::Uuid, Needs::TimeAndRandom),
        ("ulid", "parse") => (
            "parse",
            vec![Ty::Str],
            Ty::nullable(Ty::Ulid),
            Needs::Nothing,
        ),
        ("ulid", "new") => ("new", vec![], Ty::Ulid, Needs::TimeAndRandom),
        ("ulid", "at") => ("at", vec![Ty::Int], Ty::Ulid, Needs::Random),
        ("datetime", "now") => ("now", vec![], Ty::DateTime, Needs::Time),
        // 3 arguments: a day; 6: a day and a time
        ("datetime", "of") if nargs == 6 => ("of", vec![Ty::Int; 6], Ty::DateTime, Needs::Nothing),
        ("datetime", "of") => ("of", vec![Ty::Int; 3], Ty::DateTime, Needs::Nothing),
        ("datetime", "at") => ("at", vec![Ty::Int], Ty::DateTime, Needs::Nothing),
        ("datetime", "parse") => (
            "parse",
            vec![Ty::Str],
            Ty::nullable(Ty::DateTime),
            Needs::Nothing,
        ),
        ("time", "now") => ("now", vec![], Ty::Int, Needs::Time),
        ("time", "sleep") => ("sleep", vec![Ty::Int], Ty::Unit, Needs::Time),
        ("random", "int") => ("int", vec![Ty::Int, Ty::Int], Ty::Int, Needs::Random),
        ("random", "float") => ("float", vec![], Ty::Float, Needs::Random),
        ("regex", "matches") => ("matches", vec![Ty::Str, Ty::Str], Ty::Bool, Needs::Nothing),
        ("regex", "find") => (
            "find",
            vec![Ty::Str, Ty::Str],
            Ty::nullable(Ty::Str),
            Needs::Nothing,
        ),
        ("regex", "find_all") => (
            "find_all",
            vec![Ty::Str, Ty::Str],
            Ty::Array(Box::new(Ty::Str)),
            Needs::Nothing,
        ),
        ("regex", "captures") => (
            "captures",
            vec![Ty::Str, Ty::Str],
            Ty::nullable(Ty::Array(Box::new(Ty::nullable(Ty::Str)))),
            Needs::Nothing,
        ),
        ("regex", "replace") => (
            "replace",
            vec![Ty::Str, Ty::Str, Ty::Str],
            Ty::Str,
            Needs::Nothing,
        ),
        ("regex", "replace_all") => (
            "replace_all",
            vec![Ty::Str, Ty::Str, Ty::Str],
            Ty::Str,
            Needs::Nothing,
        ),
        ("regex", "split") => (
            "split",
            vec![Ty::Str, Ty::Str],
            Ty::Array(Box::new(Ty::Str)),
            Needs::Nothing,
        ),
        ("Base64", "encode") => ("encode", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Base64", "encode_url") => ("encode_url", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Base64", "decode") => (
            "decode",
            vec![Ty::Str],
            Ty::nullable(Ty::Str),
            Needs::Nothing,
        ),
        ("Hex", "encode") => ("encode", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Hex", "decode") => (
            "decode",
            vec![Ty::Str],
            Ty::nullable(Ty::Str),
            Needs::Nothing,
        ),
        ("Url", "encode") => ("encode", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Url", "decode") => (
            "decode",
            vec![Ty::Str],
            Ty::nullable(Ty::Str),
            Needs::Nothing,
        ),
        ("Hash", "sha256") => ("sha256", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Hash", "sha1") => ("sha1", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Hash", "md5") => ("md5", vec![Ty::Str], Ty::Str, Needs::Nothing),
        ("Hash", "hmac_sha256") => (
            "hmac_sha256",
            vec![Ty::Str, Ty::Str],
            Ty::Str,
            Needs::Nothing,
        ),
        _ => return None,
    })
}

fn callee_name(callee: &Expr) -> String {
    match &callee.kind {
        ExprKind::Ident(n, _) => n.clone(),
        ExprKind::Field(_, n, _) => n.name.clone(),
        _ => "function".to_string(),
    }
}

impl Checker {
    fn prelude_ty(&self, name: &str) -> Ty {
        let sid = self
            .defs
            .structs
            .iter()
            .position(|s| s.module == PRELUDE_MOD && s.name == name)
            .unwrap_or_else(|| panic!("prelude struct `{name}` not found"));
        Ty::Struct(sid, Vec::new())
    }

    /// `Reflect.struct<T>()`, `Reflect.enum<T>()`, `Reflect.functions()` and
    /// `Reflect.find<F>(name)`: the interpreter builds the answer from what the checker knows.
    fn call_reflect(
        &mut self,
        callee: &Expr,
        name: &Ident,
        targs: &[TypeExpr],
        args: &[CallArg],
    ) -> Ty {
        let arity = |ck: &mut Checker, targs_wanted: usize, args_wanted: usize| -> bool {
            if targs.len() != targs_wanted || args.len() != args_wanted {
                ck.err(
                    "T300",
                    format!(
                        "`Reflect.{}` takes {targs_wanted} type(s) between `<>` and {args_wanted} argument(s)",
                        name.name
                    ),
                    callee.span,
                );
                ck.check_args_loose(args);
                return false;
            }
            true
        };
        let op_ty = match name.name.as_str() {
            "struct_info" => {
                if !arity(self, 1, 0) {
                    return Ty::Error;
                }
                match self.resolve_type(&targs[0]) {
                    Ty::Struct(sid, _) => (ReflectOp::Struct(sid), self.prelude_ty("StructInfo")),
                    Ty::Error => return Ty::Error,
                    _ => {
                        self.err(
                            "T301",
                            "`Reflect.struct_info` needs a struct type",
                            targs[0].span,
                        );
                        return Ty::Error;
                    }
                }
            }
            "enum_info" => {
                if !arity(self, 1, 0) {
                    return Ty::Error;
                }
                match self.resolve_type(&targs[0]) {
                    Ty::Enum(eid, _) => (ReflectOp::Enum(eid), self.prelude_ty("EnumInfo")),
                    Ty::Error => return Ty::Error,
                    _ => {
                        self.err(
                            "T301",
                            "`Reflect.enum_info` needs an enum type",
                            targs[0].span,
                        );
                        return Ty::Error;
                    }
                }
            }
            "functions" => {
                if !arity(self, 0, 0) {
                    return Ty::Error;
                }
                (
                    ReflectOp::Functions(self.cur_mod),
                    Ty::Array(Box::new(self.prelude_ty("FunctionInfo"))),
                )
            }
            "methods" => {
                if !arity(self, 1, 0) {
                    return Ty::Error;
                }
                match self.resolve_type(&targs[0]) {
                    t @ (Ty::Struct(..) | Ty::Enum(..)) => (
                        ReflectOp::Methods(t),
                        Ty::Array(Box::new(self.prelude_ty("FunctionInfo"))),
                    ),
                    Ty::Error => return Ty::Error,
                    _ => {
                        self.err(
                            "T301",
                            "`Reflect.methods` needs a struct or an enum type",
                            targs[0].span,
                        );
                        return Ty::Error;
                    }
                }
            }
            "find_method" => {
                if !arity(self, 2, 2) {
                    return Ty::Error;
                }
                let t = self.resolve_type(&targs[0]);
                let f = self.resolve_type(&targs[1]);
                if t == Ty::Error || f == Ty::Error {
                    self.check_args_loose(args);
                    return Ty::Error;
                }
                if !matches!(t, Ty::Struct(..) | Ty::Enum(..)) || !matches!(f, Ty::Fn(..)) {
                    self.err(
                        "T301",
                        "`Reflect.find_method` needs a struct or enum type, then a function type (without `self`)",
                        callee.span,
                    );
                    self.check_args_loose(args);
                    return Ty::Error;
                }
                let vt = self.expr(&args[0].value, &Want::Exp(t.clone()));
                if !self.assignable(&vt, &t) {
                    self.mismatch(args[0].value.span, &t, &vt);
                }
                let nt = self.expr(&args[1].value, &Want::Exp(Ty::Str));
                if !self.assignable(&nt, &Ty::Str) {
                    self.mismatch(args[1].value.span, &Ty::Str, &nt);
                }
                (ReflectOp::FindMethod(t, f.clone()), Ty::nullable(f))
            }
            "find" => {
                if !arity(self, 1, 1) {
                    return Ty::Error;
                }
                let f = self.resolve_type(&targs[0]);
                match &f {
                    Ty::Fn(..) => {}
                    Ty::Error => return Ty::Error,
                    _ => {
                        self.err(
                            "T301",
                            "`Reflect.find` needs a function type, like `fun(int) -> int`",
                            targs[0].span,
                        );
                        return Ty::Error;
                    }
                }
                let t = self.expr(&args[0].value, &Want::Exp(Ty::Str));
                if !self.assignable(&t, &Ty::Str) {
                    self.mismatch(args[0].value.span, &Ty::Str, &t);
                }
                (ReflectOp::Find(self.cur_mod, f.clone()), Ty::nullable(f))
            }
            other => {
                self.err(
                    "T302",
                    format!("`Reflect` has no `{other}` (it has `struct_info`, `enum_info`, `functions`, `find`, `methods` and `find_method`)"),
                    name.span,
                );
                self.check_args_loose(args);
                return Ty::Error;
            }
        };
        let (op, ty) = op_ty;
        self.res.paths.insert(callee.id, PathRes::Reflect(op));
        ty
    }
}
