//! Declarations: names, signatures, impls, inheritance rules.

use super::*;
use crate::ast::{Ident, ImportKind, StmtKind, TypeExpr, TypeExprKind, TypeParam, Visibility};

const BUILTIN_TYPES: [&str; 6] = ["int", "float", "bool", "string", "array", "range"];
pub(crate) const BUILTIN_FNS: [&str; 5] = ["print", "write", "read", "panic", "assert"];

impl Checker {
    // ---------------------------------------------------------------- phase A

    fn add_item(&mut self, name: &Ident, item: ItemRef) {
        if BUILTIN_TYPES.contains(&name.name.as_str()) {
            self.err("T110", format!("`{}` is a built-in type name and cannot be redefined", name.name), name.span);
            return;
        }
        if BUILTIN_FNS.contains(&name.name.as_str()) {
            self.err("T110", format!("`{}` is a built-in function and cannot be redefined", name.name), name.span);
            return;
        }
        let m = self.cur_mod;
        if self.mscopes[m].items.contains_key(&name.name) {
            self.err("T111", format!("`{}` is already declared in this file", name.name), name.span);
            return;
        }
        self.mscopes[m].items.insert(name.name.clone(), item);
    }

    pub(crate) fn declare_module(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        let imports = self.modules[m].imports.clone();

        for (idx, item) in ast.items.iter().enumerate() {
            match item {
                Item::Fun(f) => {
                    let name = f.name.clone().unwrap();
                    let id = self.defs.fns.len();
                    self.defs.fns.push(FnDef {
                        name: name.name.clone(),
                        module: m,
                        span: f.span,
                        scope_params: Vec::new(),
                        own_tparams: 0,
                        impl_id: None,
                        has_self: false,
                        params: Vec::new(),
                        ret: Ty::Unit,
                        vis: Visibility::Public,
                        is_override: false,
                        decl: Some(f.clone()),
                    });
                    self.mscopes[m].decl_ids.insert(idx, id);
                    self.add_item(&name, ItemRef::Fn(id));
                }
                Item::Struct(s) => {
                    let id = self.defs.structs.len();
                    self.defs.structs.push(StructDef {
                        name: s.name.name.clone(),
                        module: m,
                        span: s.name.span,
                        tparams: Vec::new(),
                        parent: None,
                        fields: Vec::new(),
                    });
                    self.mscopes[m].decl_ids.insert(idx, id);
                    self.add_item(&s.name, ItemRef::Struct(id));
                }
                Item::Enum(e) => {
                    let id = self.defs.enums.len();
                    let mut variants: Vec<String> = Vec::new();
                    for v in &e.variants {
                        if variants.contains(&v.name) {
                            self.err("T112", format!("variant `{}` is declared twice in enum `{}`", v.name, e.name.name), v.span);
                        } else {
                            variants.push(v.name.clone());
                        }
                    }
                    self.defs.enums.push(EnumDef { name: e.name.name.clone(), module: m, span: e.name.span, variants });
                    self.mscopes[m].decl_ids.insert(idx, id);
                    self.add_item(&e.name, ItemRef::Enum(id));
                }
                Item::Trait(t) => {
                    let id = self.defs.traits.len();
                    self.defs.traits.push(TraitDef {
                        name: t.name.name.clone(),
                        module: m,
                        span: t.name.span,
                        tparams: Vec::new(),
                        defaults: Vec::new(),
                        sigs: Vec::new(),
                        lang: None,
                    });
                    self.mscopes[m].decl_ids.insert(idx, id);
                    self.add_item(&t.name, ItemRef::Trait(id));
                }
                Item::Stmt(s) => {
                    if let StmtKind::Let { name, .. } = &s.kind {
                        self.add_item(name, ItemRef::Var(m, name.name.clone()));
                    }
                }
                _ => {}
            }
        }

        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(&src) = imports.get(&idx) else { continue };
            match &imp.kind {
                ImportKind::Names(names) => {
                    for n in names {
                        let found = self.mscopes[src].items.get(&n.name.name).cloned();
                        let exported = self.mscopes[src].exports.contains(&n.name.name);
                        let local = n.alias.clone().unwrap_or_else(|| n.name.clone());
                        match found {
                            Some(it) if exported => self.add_item(&local, it),
                            Some(_) => self.err(
                                "T113",
                                format!("`{}` is not exported by \"{}\" (add `export {}` in that file)", n.name.name, imp.path, n.name.name),
                                n.name.span,
                            ),
                            None => self.err("T114", format!("\"{}\" has no `{}`", imp.path, n.name.name), n.name.span),
                        }
                    }
                }
                ImportKind::Module(alias) => self.add_item(alias, ItemRef::Module(src)),
            }
        }

        for item in &ast.items {
            let Item::Export(e) = item else { continue };
            for n in &e.names {
                if self.mscopes[m].items.contains_key(&n.name) {
                    self.mscopes[m].exports.insert(n.name.clone());
                } else {
                    self.err("T115", format!("cannot export `{}`: nothing with that name is declared in this file", n.name), n.span);
                }
            }
        }
    }

    // ---------------------------------------------------------- name lookup

    /// An item visible from the current module (own items, imports, prelude).
    pub(crate) fn lookup_item(&self, name: &str) -> Option<ItemRef> {
        if let Some(it) = self.mscopes[self.cur_mod].items.get(name) {
            return Some(it.clone());
        }
        for l in ALL_LANG {
            if l.trait_name() == name {
                return Some(ItemRef::Trait(self.defs.lang(l)));
            }
        }
        None
    }

    /// An exported item of module `m`.
    pub(crate) fn module_member(&mut self, m: ModId, name: &Ident) -> Option<ItemRef> {
        let it = self.mscopes[m].items.get(&name.name).cloned();
        match it {
            Some(it) if self.mscopes[m].exports.contains(&name.name) => Some(it),
            Some(_) => {
                let p = self.modules[m].path.clone();
                self.err("T113", format!("`{}` is not exported by \"{p}\"", name.name), name.span);
                None
            }
            None => {
                let p = self.modules[m].path.clone();
                self.err("T114", format!("\"{p}\" has no `{}`", name.name), name.span);
                None
            }
        }
    }

    /// Resolve `a` or `m.a` to an item, reporting unknown names.
    pub(crate) fn lookup_path_item(&mut self, path: &[Ident], what: &str) -> Option<ItemRef> {
        match path {
            [one] => {
                let r = self.lookup_item(&one.name);
                if r.is_none() {
                    self.err("T001", format!("unknown {what} `{}`", one.name), one.span);
                }
                r
            }
            [first, second] => match self.lookup_item(&first.name) {
                Some(ItemRef::Module(m)) => self.module_member(m, second),
                Some(_) => {
                    self.err("T001", format!("`{}` is not a module", first.name), first.span);
                    None
                }
                None => {
                    self.err("T001", format!("unknown module `{}`", first.name), first.span);
                    None
                }
            },
            _ => {
                self.err("T001", format!("invalid {what} path"), path[0].span.to(path[path.len() - 1].span));
                None
            }
        }
    }

    // ---------------------------------------------------------------- types

    pub(crate) fn resolve_type(&mut self, te: &TypeExpr) -> Ty {
        match &te.kind {
            TypeExprKind::Nullable(inner) => {
                let t = self.resolve_type(inner);
                if t == Ty::Unit {
                    self.err("T120", "a type without value cannot be nullable", te.span);
                    return Ty::Error;
                }
                Ty::nullable(t)
            }
            TypeExprKind::Fun { params, ret } => {
                let ps = params.iter().map(|p| self.resolve_type(p)).collect();
                let r = ret.as_ref().map(|r| self.resolve_type(r)).unwrap_or(Ty::Unit);
                Ty::Fn(ps, Box::new(r))
            }
            TypeExprKind::Named { path, args } => self.resolve_named_type(path, args, te.span),
        }
    }

    fn resolve_named_type(&mut self, path: &[Ident], args: &[TypeExpr], span: Span) -> Ty {
        if path.len() == 1 {
            let name = path[0].name.as_str();
            if let Some((_, p)) = self.tparam_scope.iter().rev().find(|(n, _)| n == name) {
                let p = *p;
                if !args.is_empty() {
                    self.err("T121", format!("type parameter `{name}` takes no type arguments"), span);
                }
                return Ty::Param(p);
            }
            let prim = match name {
                "int" => Some(Ty::Int),
                "float" => Some(Ty::Float),
                "bool" => Some(Ty::Bool),
                "string" => Some(Ty::Str),
                "range" => Some(Ty::Range),
                _ => None,
            };
            if let Some(t) = prim {
                if !args.is_empty() {
                    self.err("T121", format!("`{name}` takes no type arguments"), span);
                }
                return t;
            }
            if name == "array" {
                if args.len() != 1 {
                    self.err("T121", "`array` takes exactly one type argument: `array<int>`", span);
                    return Ty::Error;
                }
                let t = self.resolve_type(&args[0]);
                return Ty::Array(Box::new(t));
            }
        }
        let Some(item) = self.lookup_path_item(path, "type") else { return Ty::Error };
        self.item_to_type(item, path, args, span)
    }

    pub(crate) fn item_to_type(&mut self, item: ItemRef, path: &[Ident], args: &[TypeExpr], span: Span) -> Ty {
        let name = path.last().map(|i| i.name.clone()).unwrap_or_default();
        let targs: Vec<Ty> = args.iter().map(|a| self.resolve_type(a)).collect();
        match item {
            ItemRef::Struct(id) => {
                let n = self.defs.structs[id].tparams.len();
                if targs.len() != n {
                    self.err("T121", format!("`{name}` expects {n} type argument(s), found {}", targs.len()), span);
                    return Ty::Error;
                }
                Ty::Struct(id, targs)
            }
            ItemRef::Enum(id) => {
                if !targs.is_empty() {
                    self.err("T121", format!("`{name}` takes no type arguments"), span);
                }
                Ty::Enum(id)
            }
            ItemRef::Trait(id) => {
                let n = self.defs.traits[id].tparams.len();
                if targs.len() != n {
                    self.err("T121", format!("trait `{name}` expects {n} type argument(s), found {}", targs.len()), span);
                    return Ty::Error;
                }
                Ty::Trait(id, targs)
            }
            _ => {
                self.err("T001", format!("`{name}` is not a type"), span);
                Ty::Error
            }
        }
    }

    /// Resolve a trait reference (`Add`, `Iter<int>`), filling defaults with `self_ty`.
    pub(crate) fn resolve_trait_ref(&mut self, te: &TypeExpr, self_ty: &Ty) -> Option<(TraitId, Vec<Ty>)> {
        let TypeExprKind::Named { path, args } = &te.kind else {
            self.err("T122", "expected a trait here", te.span);
            return None;
        };
        let item = self.lookup_path_item(path, "trait")?;
        let ItemRef::Trait(id) = item else {
            self.err("T122", format!("`{}` is not a trait", path.last().unwrap().name), te.span);
            return None;
        };
        let given: Vec<Ty> = args.iter().map(|a| self.resolve_type(a)).collect();
        let tparams = self.defs.traits[id].tparams.len();
        if given.len() > tparams {
            self.err("T121", format!("trait `{}` expects at most {tparams} type argument(s)", self.defs.traits[id].name), te.span);
            return None;
        }
        let mut out = given.clone();
        let self_map: HashMap<ParamId, Ty> = [(SELF_PARAM, self_ty.clone())].into_iter().collect();
        for i in given.len()..tparams {
            match self.defs.traits[id].defaults[i].clone() {
                Some(d) => out.push(d.subst(&self_map)),
                None => {
                    self.err(
                        "T121",
                        format!("trait `{}` needs {tparams} type argument(s): `{}<...>`", self.defs.traits[id].name, self.defs.traits[id].name),
                        te.span,
                    );
                    return None;
                }
            }
        }
        Some((id, out))
    }

    /// Create the generic parameters of a declaration and bring them in scope.
    pub(crate) fn make_tparams(&mut self, decls: &[TypeParam]) -> Vec<(String, ParamId)> {
        let mut out: Vec<(String, ParamId)> = Vec::new();
        for d in decls {
            if out.iter().any(|(n, _)| *n == d.name.name) {
                self.err("T123", format!("type parameter `{}` is declared twice", d.name.name), d.name.span);
                continue;
            }
            let id = self.defs.new_param(&d.name.name);
            out.push((d.name.name.clone(), id));
            self.tparam_scope.push((d.name.name.clone(), id));
        }
        for d in decls {
            let Some((_, id)) = out.iter().find(|(n, _)| *n == d.name.name).cloned() else { continue };
            for b in &d.bounds {
                if let Some(bound) = self.resolve_trait_ref(b, &Ty::Param(id)) {
                    self.defs.params[id].bounds.push(bound);
                }
            }
        }
        out
    }

    // ---------------------------------------------------------------- phase B

    pub(crate) fn resolve_traits(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Trait(t) = item else { continue };
            let id = self.mscopes[m].decl_ids[&idx];
            self.tparam_scope.clear();
            let tps = self.make_tparams(&t.tparams);
            let mut sigs: Vec<SigDef> = Vec::new();
            for s in &t.sigs {
                let name = s.name.clone().unwrap();
                if sigs.iter().any(|x| x.name == name.name) {
                    self.err("T124", format!("method `{}` is declared twice in trait `{}`", name.name, t.name.name), name.span);
                    continue;
                }
                let mark = self.tparam_scope.len();
                let own = self.make_tparams(&s.tparams);
                let params = s.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
                let ret = s.ret.as_ref().map(|r| self.resolve_type(r)).unwrap_or(Ty::Unit);
                self.tparam_scope.truncate(mark);
                sigs.push(SigDef {
                    name: name.name,
                    tparams: own.into_iter().map(|(_, p)| p).collect(),
                    has_self: s.has_self,
                    params,
                    ret: Some(ret),
                });
            }
            let tp_ids: Vec<ParamId> = tps.iter().map(|(_, p)| *p).collect();
            let n = tp_ids.len();
            let def = &mut self.defs.traits[id];
            def.tparams = tp_ids;
            def.defaults = vec![None; n];
            def.sigs = sigs;
            self.tparam_scope.clear();
        }
    }

    pub(crate) fn resolve_structs(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Struct(s) = item else { continue };
            let id = self.mscopes[m].decl_ids[&idx];
            self.tparam_scope.clear();
            let tps = self.make_tparams(&s.tparams);
            self.defs.structs[id].tparams = tps.iter().map(|(_, p)| *p).collect();
            if let Some(pt) = &s.parent {
                match self.resolve_type(pt) {
                    Ty::Struct(pid, args) => self.defs.structs[id].parent = Some((pid, args)),
                    Ty::Error => {}
                    other => {
                        let shown = self.show(&other);
                        self.err("T125", format!("a struct can only extend another struct, not `{shown}`"), pt.span);
                    }
                }
            }
            let mut fields: Vec<FieldDef> = Vec::new();
            for f in &s.fields {
                if fields.iter().any(|x| x.name == f.name.name) {
                    self.err("T126", format!("field `{}` is declared twice in `{}`", f.name.name, s.name.name), f.name.span);
                    continue;
                }
                let ty = self.resolve_type(&f.ty);
                if f.is_static && f.default.is_none() {
                    self.err(
                        "T127",
                        format!("static field `{0}` needs an initial value (`static {0}: type = value`)", f.name.name),
                        f.name.span,
                    );
                }
                fields.push(FieldDef {
                    name: f.name.name.clone(),
                    ty,
                    vis: f.vis.unwrap_or(Visibility::Private),
                    is_static: f.is_static,
                    default: f.default.clone(),
                    span: f.name.span,
                });
            }
            self.defs.structs[id].fields = fields;
            self.tparam_scope.clear();
        }
    }

    pub(crate) fn resolve_functions(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Fun(f) = item else { continue };
            let id = self.mscopes[m].decl_ids[&idx];
            self.tparam_scope.clear();
            let own = self.make_tparams(&f.tparams);
            if f.has_self {
                self.err("T127", "`self` is only available in methods", f.span);
            }
            let params = f.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
            let ret = f.ret.as_ref().map(|r| self.resolve_type(r)).unwrap_or(Ty::Unit);
            let def = &mut self.defs.fns[id];
            def.own_tparams = own.len();
            def.scope_params = own;
            def.params = params;
            def.ret = ret;
            self.tparam_scope.clear();
        }
    }

    pub(crate) fn resolve_impls(&mut self, m: ModId) {
        let ast = self.modules[m].ast.clone();
        for (item_idx, item) in ast.items.iter().enumerate() {
            let Item::Impl(imp) = item else { continue };
            self.tparam_scope.clear();
            let tps = self.make_tparams(&imp.tparams);
            let target = self.resolve_type(&imp.target);
            match &target {
                Ty::Error => {
                    self.tparam_scope.clear();
                    continue;
                }
                Ty::Struct(..) | Ty::Enum(_) | Ty::Int | Ty::Float | Ty::Bool | Ty::Str | Ty::Array(_) => {}
                other => {
                    let shown = self.show(other);
                    self.err("T130", format!("cannot implement methods for `{shown}`"), imp.target.span);
                    self.tparam_scope.clear();
                    continue;
                }
            }
            let trait_ = match &imp.trait_ref {
                Some(te) => match self.resolve_trait_ref(te, &target) {
                    Some(t) => Some(t),
                    None => {
                        self.tparam_scope.clear();
                        continue;
                    }
                },
                None => None,
            };
            let impl_id = self.defs.impls.len();
            self.mscopes[m].decl_ids.insert(item_idx, impl_id);
            self.defs.impls.push(ImplDef {
                module: m,
                span: imp.span,
                tparams: tps.clone(),
                trait_: trait_.clone(),
                target: target.clone(),
                methods: Vec::new(),
            });

            let mut methods: Vec<(String, FnId)> = Vec::new();
            for md in &imp.methods {
                let f = &md.fun;
                let name = f.name.clone().unwrap();
                if methods.iter().any(|(n, _)| *n == name.name) {
                    self.err("T131", format!("method `{}` is declared twice in this impl", name.name), name.span);
                    continue;
                }
                let mark = self.tparam_scope.len();
                let own = self.make_tparams(&f.tparams);
                let params: Vec<Ty> = f.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
                let ret = f.ret.as_ref().map(|r| self.resolve_type(r)).unwrap_or(Ty::Unit);
                self.tparam_scope.truncate(mark);
                let mut scope_params = tps.clone();
                scope_params.extend(own.iter().cloned());
                let fid = self.defs.fns.len();
                self.defs.fns.push(FnDef {
                    name: name.name.clone(),
                    module: m,
                    span: name.span,
                    scope_params,
                    own_tparams: own.len(),
                    impl_id: Some(impl_id),
                    has_self: f.has_self,
                    params,
                    ret,
                    vis: md.vis.unwrap_or(Visibility::Private),
                    is_override: md.is_override,
                    decl: Some(f.clone()),
                });
                methods.push((name.name.clone(), fid));
            }
            self.defs.impls[impl_id].methods = methods;
            self.verify_impl(impl_id, imp);
            self.tparam_scope.clear();
        }
    }

    /// Check an impl against its trait, or for duplicate inherent methods.
    fn verify_impl(&mut self, impl_id: ImplId, imp: &ast::ImplDecl) {
        let def = self.defs.impls[impl_id].clone();
        let Some((tid, targs)) = def.trait_.clone() else {
            // inherent: names must not clash with other inherent impls of the same type
            for (name, fid) in &def.methods {
                for (oi, other) in self.defs.impls.clone().iter().enumerate() {
                    if oi == impl_id || other.trait_.is_some() || other.target != def.target {
                        continue;
                    }
                    if other.methods.iter().any(|(n, _)| n == name) {
                        let sp = self.defs.fns[*fid].span;
                        self.err("T131", format!("method `{name}` is already defined for this type"), sp);
                    }
                }
                if self.defs.fns[*fid].is_override && !self.defs.fns[*fid].has_self {
                    let sp = self.defs.fns[*fid].span;
                    self.err("T132", "only methods with `self` can be overridden", sp);
                }
            }
            return;
        };
        let tdef = self.defs.traits[tid].clone();
        let mut map: HashMap<ParamId, Ty> = HashMap::new();
        map.insert(SELF_PARAM, def.target.clone());
        for (p, a) in tdef.tparams.iter().zip(targs.iter()) {
            map.insert(*p, a.clone());
        }
        let trait_shown = self.show(&Ty::Trait(tid, targs.clone()));
        for sig in &tdef.sigs {
            let Some((_, fid)) = def.methods.iter().find(|(n, _)| *n == sig.name).cloned() else {
                self.err(
                    "T133",
                    format!("missing method `{}` required by trait `{trait_shown}`", sig.name),
                    imp.target.span,
                );
                continue;
            };
            let f = self.defs.fns[fid].clone();
            if f.has_self != sig.has_self {
                self.err(
                    "T134",
                    format!("method `{}` should {} `self`", sig.name, if sig.has_self { "take" } else { "not take" }),
                    f.span,
                );
                continue;
            }
            // map the signature's own generics onto the method's own generics
            let mut m2 = map.clone();
            let own: Vec<ParamId> = f.scope_params[f.scope_params.len() - f.own_tparams..].iter().map(|(_, p)| *p).collect();
            if own.len() != sig.tparams.len() {
                self.err("T134", format!("method `{}` should have {} generic parameter(s)", sig.name, sig.tparams.len()), f.span);
                continue;
            }
            for (sp, mp) in sig.tparams.iter().zip(own.iter()) {
                m2.insert(*sp, Ty::Param(*mp));
            }
            if f.params.len() != sig.params.len() {
                self.err(
                    "T134",
                    format!("method `{}` should take {} parameter(s), found {}", sig.name, sig.params.len(), f.params.len()),
                    f.span,
                );
                continue;
            }
            for (i, (have, want)) in f.params.iter().zip(sig.params.iter()).enumerate() {
                let want = want.subst(&m2);
                if !self.same(have, &want) {
                    let (h, w) = (self.show(have), self.show(&want));
                    self.err("T134", format!("parameter {} of `{}` should have type `{w}`, found `{h}`", i + 1, sig.name), f.span);
                }
            }
            if let Some(r) = &sig.ret {
                let want = r.subst(&m2);
                if !self.assignable(&f.ret, &want) {
                    let (h, w) = (self.show(&f.ret), self.show(&want));
                    self.err("T134", format!("method `{}` should return `{w}`, found `{h}`", sig.name), f.span);
                }
            }
            if f.is_override {
                self.err("T132", "`override` applies to methods of a struct, not to trait implementations", f.span);
            }
        }
        for (name, fid) in &def.methods {
            if !tdef.sigs.iter().any(|s| s.name == *name) {
                let sp = self.defs.fns[*fid].span;
                self.err("T135", format!("`{name}` is not a method of trait `{trait_shown}`"), sp);
            }
        }
    }

    pub(crate) fn validate_structs(&mut self) {
        for sid in 0..self.defs.structs.len() {
            // cycles in the parent chain
            let mut cur = sid;
            let mut seen = vec![sid];
            let mut cyclic = false;
            while let Some((p, _)) = self.defs.structs[cur].parent.clone() {
                if seen.contains(&p) {
                    cyclic = true;
                    break;
                }
                seen.push(p);
                cur = p;
            }
            if cyclic {
                let name = self.defs.structs[sid].name.clone();
                let sp = self.defs.structs[sid].span;
                self.err("T140", format!("struct `{name}` cannot extend itself, directly or not"), sp);
                self.defs.structs[sid].parent = None;
                continue;
            }
            // a field cannot reuse an inherited field name
            let anc = self.defs.ancestry(sid, &[]);
            for (aid, _) in anc.iter().skip(1) {
                let parent_fields: Vec<String> = self.defs.structs[*aid].fields.iter().map(|f| f.name.clone()).collect();
                let own: Vec<(String, Span)> = self.defs.structs[sid].fields.iter().map(|f| (f.name.clone(), f.span)).collect();
                for (n, sp) in own {
                    if parent_fields.contains(&n) {
                        let pn = self.defs.structs[*aid].name.clone();
                        self.err("T126", format!("field `{n}` already exists in parent struct `{pn}`"), sp);
                    }
                }
            }
        }
    }

    pub(crate) fn validate_overrides(&mut self) {
        for iid in 0..self.defs.impls.len() {
            let imp = self.defs.impls[iid].clone();
            if imp.trait_.is_some() {
                continue;
            }
            let Ty::Struct(sid, args) = imp.target.clone() else { continue };
            let anc = self.defs.ancestry(sid, &args);
            for (name, fid) in imp.methods.clone() {
                let f = self.defs.fns[fid].clone();
                if !f.has_self {
                    continue;
                }
                let mut parent_method: Option<(FnId, StructId)> = None;
                'outer: for (aid, _) in anc.iter().skip(1) {
                    for other in self.defs.impls.iter() {
                        if other.trait_.is_some() {
                            continue;
                        }
                        if let Ty::Struct(osid, _) = other.target
                            && osid == *aid
                                && let Some((_, pf)) = other.methods.iter().find(|(n, _)| *n == name)
                                    && self.defs.fns[*pf].has_self {
                                        parent_method = Some((*pf, *aid));
                                        break 'outer;
                                    }
                    }
                }
                match (parent_method, f.is_override) {
                    (Some((_, aid)), false) => {
                        let pn = self.defs.structs[aid].name.clone();
                        self.err("T132", format!("method `{name}` redefines a method of `{pn}`: add `override`"), f.span);
                    }
                    (None, true) => {
                        self.err("T132", format!("`override` on `{name}`, but no parent struct has this method"), f.span);
                    }
                    (Some((pf, _)), true) => {
                        let pdef = self.defs.fns[pf].clone();
                        let same_params = pdef.params.len() == f.params.len();
                        if !same_params {
                            self.err("T132", format!("override of `{name}` must take {} parameter(s)", pdef.params.len()), f.span);
                        } else if !self.assignable(&f.ret, &pdef.ret) {
                            let (a, b) = (self.show(&f.ret), self.show(&pdef.ret));
                            self.err("T132", format!("override of `{name}` must return `{b}`, found `{a}`"), f.span);
                        }
                    }
                    (None, false) => {}
                }
            }
        }
    }
}
