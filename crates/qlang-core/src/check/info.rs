//! What the checker learned about a program, for editors: the type of every
//! expression, the local variables, and the members of every type. It is only collected
//! when asked for (`CompileOptions::info`).

use super::*;
use crate::ast::{Item, Visibility};
use crate::span::Location;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Serialize, Default)]
pub struct Info {
    /// Every expression with a value (or a type name used as `Type.member`).
    pub exprs: Vec<ExprInfo>,
    /// Variables and parameters, with the region where they can be used.
    pub locals: Vec<LocalInfo>,
    /// Definition sites: functions, types, methods, fields, top-level variables.
    pub items: Vec<ItemInfo>,
    /// Names usable at the top level of each file (own and imported), by file name.
    pub globals: BTreeMap<String, Vec<Member>>,
    /// The functions that exist everywhere (`print`…).
    pub functions: Vec<Member>,
    /// `value.member`, by the type of `value`.
    pub members: BTreeMap<String, Vec<Member>>,
    /// `Type.member` and `module.member`, by the key found in [`ExprInfo::statics`].
    pub statics: BTreeMap<String, Vec<Member>>,
}

#[derive(Serialize)]
pub struct ExprInfo {
    pub loc: Location,
    #[serde(rename = "type")]
    pub ty: String,
    /// What a name stands for (`fun add(a: int) -> int`, `count: int`…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Set when the expression is a type or a module: the key of its `statics`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statics: Option<String>,
    /// The documentation comment of the function or type this name stands for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Serialize)]
pub struct LocalInfo {
    pub loc: Location,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub kind: &'static str,
    /// Where the variable can be used (from its declaration to the end of the function).
    pub scope: Location,
}

#[derive(Serialize)]
pub struct ItemInfo {
    pub loc: Location,
    pub name: String,
    pub kind: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct Member {
    pub name: String,
    pub kind: &'static str,
    pub detail: String,
    /// Where it is declared, for user-defined members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<Location>,
    /// The explanation: the documentation comment of a user-defined member, or the text of
    /// the standard library for a built-in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub public: bool,
}

// ---------------------------------------------------------------- collection

pub(crate) enum StaticKey {
    Type(TypeRef),
    Module(ModId),
}

struct RawExpr {
    span: Span,
    ty: Ty,
    id: ast::NodeId,
    name: Option<String>,
    statics: Option<String>,
}

struct RawLocal {
    name: String,
    span: Span,
    ty: Ty,
    kind: LocalKind,
    scope: Span,
}

#[derive(Default)]
pub(crate) struct Collector {
    exprs: Vec<RawExpr>,
    locals: Vec<RawLocal>,
    statics: BTreeMap<String, StaticKey>,
}

impl Collector {
    pub fn expr(&mut self, e: &ast::Expr, ty: Ty) {
        let name = match &e.kind {
            ast::ExprKind::Ident(n, _) => Some(n.clone()),
            ast::ExprKind::SelfVal => Some("self".to_string()),
            _ => None,
        };
        self.exprs.push(RawExpr { span: e.span, ty, id: e.id, name, statics: None });
    }

    pub fn head(&mut self, e: &ast::Expr, ty: Ty, key: String, what: StaticKey) {
        self.exprs.push(RawExpr { span: e.span, ty, id: e.id, name: None, statics: Some(key.clone()) });
        self.statics.entry(key).or_insert(what);
    }

    pub fn local(&mut self, name: &ast::Ident, local: &Local, scope: Span) {
        if name.name == "self" || name.name == "_" || local.orig.is_some() {
            return;
        }
        self.locals.push(RawLocal { name: name.name.clone(), span: name.span, ty: local.ty.clone(), kind: local.kind, scope });
    }
}

// ---------------------------------------------------------------------- docs

/// The documentation comments of all the modules, found by the position of the name they document.
#[derive(Default)]
pub(crate) struct Docs {
    /// (file, start of the name): the comment of a function, type, field or constant.
    by_name: HashMap<(u32, u32), String>,
    /// (file, start of the enum name): the comment of each variant.
    variants: HashMap<(u32, u32), Vec<Option<String>>>,
    /// (file, start of the trait name): the comment of each method signature.
    signatures: HashMap<(u32, u32), HashMap<String, String>>,
}

impl Docs {
    fn collect(modules: &[Loaded]) -> Docs {
        let mut d = Docs::default();
        let key = |s: Span| (s.file, s.start);
        for module in modules {
            for item in &module.ast.items {
                match item {
                    Item::Fun(f) => d.add_fun(f),
                    Item::Struct(s) => {
                        d.put(key(s.name.span), &s.doc);
                        for f in &s.fields {
                            d.put(key(f.name.span), &f.doc);
                        }
                    }
                    Item::Enum(e) => {
                        d.put(key(e.name.span), &e.doc);
                        d.variants.insert(key(e.name.span), e.variant_docs.iter().map(|x| x.as_ref().map(|t| t.to_string())).collect());
                    }
                    Item::Trait(t) => {
                        d.put(key(t.name.span), &t.doc);
                        let sigs = t.sigs.iter().filter_map(|f| Some((f.name.as_ref()?.name.clone(), f.doc.as_ref()?.to_string()))).collect();
                        d.signatures.insert(key(t.name.span), sigs);
                    }
                    Item::Impl(i) => {
                        for m in &i.methods {
                            d.add_fun(&m.fun);
                        }
                    }
                    Item::Stmt(ast::Stmt { kind: ast::StmtKind::Let { doc, name, .. }, .. }) => d.put(key(name.span), doc),
                    _ => {}
                }
            }
        }
        d
    }

    fn put(&mut self, at: (u32, u32), doc: &Option<Rc<str>>) {
        if let Some(text) = doc {
            self.by_name.insert(at, text.to_string());
        }
    }

    fn add_fun(&mut self, f: &ast::FunDecl) {
        if let Some(name) = &f.name {
            self.put((name.span.file, name.span.start), &f.doc);
        }
    }

    fn of(&self, span: Span) -> Option<String> {
        self.by_name.get(&(span.file, span.start)).cloned()
    }
}

// --------------------------------------------------------------------- build

fn plain(vis: Visibility) -> bool {
    vis == Visibility::Public
}

impl Checker {
    pub(crate) fn collecting(&self) -> bool {
        self.collect.is_some()
    }

    fn loc(&self, span: Span) -> Location {
        self.sources.locate(span)
    }

    /// `name(a: int, b: int) -> int`
    fn signature(&self, name: &str, names: &[String], params: &[Ty], ret: &Ty, map: &HashMap<ParamId, Ty>) -> String {
        let shown: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let t = self.show(&p.subst(map));
                match names.get(i) {
                    Some(n) if !n.is_empty() => format!("{n}: {t}"),
                    _ => t,
                }
            })
            .collect();
        let ret = ret.subst(map);
        if matches!(ret, Ty::Unit) {
            format!("{name}({})", shown.join(", "))
        } else {
            format!("{name}({}) -> {}", shown.join(", "), self.show(&ret))
        }
    }

    fn fn_signature(&self, fid: FnId, map: &HashMap<ParamId, Ty>) -> String {
        let f = &self.defs.fns[fid];
        let names: Vec<String> = f.decl.as_ref().map(|d| d.params.iter().map(|p| p.name.name.clone()).collect()).unwrap_or_default();
        self.signature(&f.name, &names, &f.params, &f.ret, map)
    }

    fn fn_member(&self, fid: FnId, map: &HashMap<ParamId, Ty>) -> Member {
        let f = &self.defs.fns[fid];
        let name = f.decl.as_ref().and_then(|d| d.name.as_ref());
        let loc = name.map(|n| self.loc(n.span));
        Member {
            name: f.name.clone(),
            kind: "method",
            detail: self.fn_signature(fid, map),
            loc,
            doc: name.and_then(|n| self.docs.of(n.span)),
            public: plain(f.vis),
        }
    }

    fn describe(&self, item: &ItemRef) -> (&'static str, String, Option<Location>, Option<String>) {
        let fn_doc = |f: &FnId| self.defs.fns[*f].decl.as_ref().and_then(|d| d.name.as_ref()).and_then(|n| self.docs.of(n.span));
        match item {
            ItemRef::Fn(f) => ("function", format!("fun {}", self.fn_signature(*f, &HashMap::new())), self.fn_name_loc(*f), fn_doc(f)),
            ItemRef::Struct(s) => ("struct", format!("struct {}", self.defs.structs[*s].name), None, self.docs.of(self.defs.structs[*s].span)),
            ItemRef::Enum(e) => ("enum", format!("enum {}", self.defs.enums[*e].name), None, self.docs.of(self.defs.enums[*e].span)),
            ItemRef::Trait(t) => ("trait", format!("trait {}", self.defs.traits[*t].name), None, self.docs.of(self.defs.traits[*t].span)),
            ItemRef::Var(m, n) => {
                let local = self.mscopes[*m].vars.get(n);
                let t = local.map(|l| self.show(&l.ty)).unwrap_or_default();
                ("variable", format!("{n}: {t}"), None, local.and_then(|l| self.docs.of(l.span)))
            }
            ItemRef::Module(m) => ("module", format!("module \"{}\"", self.modules[*m].path), None, None),
        }
    }

    fn fn_name_loc(&self, fid: FnId) -> Option<Location> {
        self.defs.fns[fid].decl.as_ref().and_then(|d| d.name.as_ref()).map(|n| self.loc(n.span))
    }

    fn item_members(&self, m: ModId, only_exported: bool) -> Vec<Member> {
        let mut out = Vec::new();
        let mut names: Vec<&String> = self.mscopes[m].items.keys().collect();
        names.sort();
        for name in names {
            if only_exported && !self.mscopes[m].exports.contains(name) {
                continue;
            }
            let item = &self.mscopes[m].items[name];
            let (kind, detail, loc, doc) = self.describe(item);
            out.push(Member { name: name.clone(), kind, detail, loc, doc, public: true });
        }
        out
    }

    /// What `value.` offers for a value of type `ty`.
    fn members_of(&mut self, ty: &Ty) -> Vec<Member> {
        let mut out: Vec<Member> = Vec::new();
        let builtin = |this: &mut Checker, owner: &str, out: &mut Vec<Member>| {
            for (name, pnames, doc) in catalog::methods(owner) {
                let Some((params, ret)) = this.builtin_method(ty, name) else { continue };
                let names: Vec<String> = pnames.split(',').map(|s| s.trim().to_string()).collect();
                out.push(Member {
                    name: name.to_string(),
                    kind: "method",
                    detail: this.signature(name, &names, &params, &ret, &HashMap::new()),
                    loc: None,
                    doc: Some(doc.to_string()),
                    public: true,
                });
            }
        };
        match ty {
            Ty::Array(_) => builtin(self, "array", &mut out),
            Ty::Map(..) => builtin(self, "map", &mut out),
            Ty::Str => builtin(self, "string", &mut out),
            Ty::Uuid => builtin(self, "uuid", &mut out),
            Ty::Ulid => builtin(self, "ulid", &mut out),
            Ty::Int => builtin(self, "int", &mut out),
            Ty::Float => builtin(self, "float", &mut out),
            Ty::Struct(sid, args) => {
                for (asid, aargs) in self.defs.ancestry(*sid, args) {
                    let def = self.defs.structs[asid].clone();
                    let map: HashMap<ParamId, Ty> = def.tparams.iter().cloned().zip(aargs.iter().cloned()).collect();
                    for f in def.fields.iter().filter(|f| !f.is_static) {
                        out.push(Member {
                            name: f.name.clone(),
                            kind: "field",
                            detail: format!("{}: {}", f.name, self.show(&f.ty.subst(&map))),
                            loc: Some(self.loc(f.span)),
                            doc: self.docs.of(f.span),
                            public: plain(f.vis),
                        });
                    }
                    self.impl_methods(&Ty::Struct(asid, aargs), true, &mut out);
                }
            }
            Ty::Enum(_) => self.impl_methods(ty, true, &mut out),
            Ty::Trait(tid, _) => self.trait_methods(*tid, &mut out),
            Ty::Param(p) => {
                let bounds = self.defs.params[*p].bounds.clone();
                for (tid, _) in bounds {
                    self.trait_methods(tid, &mut out);
                }
            }
            _ => {}
        }
        dedup(out)
    }

    fn trait_methods(&self, tid: TraitId, out: &mut Vec<Member>) {
        let t = &self.defs.traits[tid];
        if t.lang.is_some() {
            return;
        }
        let docs = self.docs.signatures.get(&(t.span.file, t.span.start));
        for sig in t.sigs.iter().filter(|s| s.has_self) {
            let ret = sig.ret.clone().unwrap_or(Ty::Unit);
            out.push(Member {
                name: sig.name.clone(),
                kind: "method",
                detail: self.signature(&sig.name, &[], &sig.params, &ret, &HashMap::new()),
                loc: None,
                doc: docs.and_then(|d| d.get(&sig.name)).cloned(),
                public: true,
            });
        }
    }

    /// The methods of the impls whose target is (the head of) `ty`; `with_self` picks
    /// instance methods or static ones.
    fn impl_methods(&self, ty: &Ty, with_self: bool, out: &mut Vec<Member>) {
        // inherent impls first, so that they win over trait impls of the same name
        for pass in 0..2 {
            for imp in &self.defs.impls {
                if (pass == 0) != imp.trait_.is_none() {
                    continue;
                }
                if let Some((tid, _)) = &imp.trait_
                    && self.defs.traits[*tid].lang.is_some()
                {
                    continue;
                }
                let map = match (&imp.target, ty) {
                    (Ty::Struct(a, targs), Ty::Struct(b, args)) if a == b => {
                        targs.iter().zip(args.iter()).filter_map(|(t, v)| if let Ty::Param(p) = t { Some((*p, v.clone())) } else { None }).collect()
                    }
                    (Ty::Enum(a), Ty::Enum(b)) if a == b => HashMap::new(),
                    _ => continue,
                };
                for (_, fid) in &imp.methods {
                    if self.defs.fns[*fid].has_self == with_self {
                        out.push(self.fn_member(*fid, &map));
                    }
                }
            }
        }
    }

    /// What `Type.` or `module.` offers.
    fn statics_of(&mut self, key: &StaticKey) -> Vec<Member> {
        let mut out = Vec::new();
        match key {
            StaticKey::Module(m) => out = self.item_members(*m, true),
            StaticKey::Type(TypeRef::Builtin(b)) => {
                for (owner, kind, name, detail, doc) in catalog::STATICS {
                    if owner == b {
                        out.push(Member { name: name.to_string(), kind, detail: detail.to_string(), loc: None, doc: Some(doc.to_string()), public: true });
                    }
                }
            }
            StaticKey::Type(TypeRef::Enum(eid)) => {
                let def = self.defs.enums[*eid].clone();
                let docs = self.docs.variants.get(&(def.span.file, def.span.start));
                for (i, v) in def.variants.iter().enumerate() {
                    out.push(Member {
                        name: v.clone(),
                        kind: "variant",
                        detail: format!("{}.{v}", def.name),
                        loc: None,
                        doc: docs.and_then(|d| d.get(i)).cloned().flatten(),
                        public: true,
                    });
                }
                self.impl_methods(&Ty::Enum(*eid), false, &mut out);
            }
            StaticKey::Type(TypeRef::Struct(sid)) => {
                for (asid, _) in self.defs.ancestry(*sid, &[]) {
                    let def = self.defs.structs[asid].clone();
                    for f in def.fields.iter().filter(|f| f.is_static) {
                        out.push(Member {
                            name: f.name.clone(),
                            kind: "constant",
                            detail: format!("{}: {}", f.name, self.show(&f.ty)),
                            loc: Some(self.loc(f.span)),
                            doc: self.docs.of(f.span),
                            public: plain(f.vis),
                        });
                    }
                    let params: Vec<Ty> = def.tparams.iter().map(|p| Ty::Param(*p)).collect();
                    self.impl_methods(&Ty::Struct(asid, params), false, &mut out);
                }
            }
        }
        dedup(out)
    }

    /// Items defined by the source files, by position.
    fn definition_sites(&self) -> Vec<ItemInfo> {
        let mut out = Vec::new();
        let mut push = |span: Span, name: &str, kind: &'static str, label: String, doc: Option<String>| {
            out.push(ItemInfo { loc: self.loc(span), name: name.to_string(), kind, label, doc });
        };
        for (m, module) in self.modules.iter().enumerate() {
            for (idx, item) in module.ast.items.iter().enumerate() {
                let id = self.mscopes[m].decl_ids.get(&idx).copied();
                match item {
                    Item::Fun(f) => {
                        if let (Some(n), Some(fid)) = (&f.name, id) {
                            push(n.span, &n.name, "function", format!("fun {}", self.fn_signature(fid, &HashMap::new())), self.docs.of(n.span));
                        }
                    }
                    Item::Struct(s) => {
                        push(s.name.span, &s.name.name, "struct", format!("struct {}", s.name.name), self.docs.of(s.name.span));
                        if let Some(sid) = id {
                            for f in &self.defs.structs[sid].fields {
                                let kind = if f.is_static { "constant" } else { "field" };
                                push(f.span, &f.name, kind, format!("{}: {}", f.name, self.show(&f.ty)), self.docs.of(f.span));
                            }
                        }
                    }
                    Item::Enum(e) => {
                        push(e.name.span, &e.name.name, "enum", format!("enum {}", e.name.name), self.docs.of(e.name.span));
                        for (i, v) in e.variants.iter().enumerate() {
                            let doc = e.variant_docs.get(i).cloned().flatten().map(|d| d.to_string());
                            push(v.span, &v.name, "variant", format!("{}.{}", e.name.name, v.name), doc);
                        }
                    }
                    Item::Trait(t) => push(t.name.span, &t.name.name, "trait", format!("trait {}", t.name.name), self.docs.of(t.name.span)),
                    Item::Impl(_) => {
                        if let Some(iid) = id {
                            for (_, fid) in &self.defs.impls[iid].methods {
                                let f = &self.defs.fns[*fid];
                                if let Some(n) = f.decl.as_ref().and_then(|d| d.name.as_ref()) {
                                    push(n.span, &f.name, "method", format!("fun {}", self.fn_signature(*fid, &HashMap::new())), self.docs.of(n.span));
                                }
                            }
                        }
                    }
                    Item::Stmt(ast::Stmt { kind: ast::StmtKind::Let { name, .. }, .. }) => {
                        if let Some(local) = self.mscopes[m].vars.get(&name.name) {
                            let kind = if local.kind == LocalKind::Const { "constant" } else { "variable" };
                            push(name.span, &name.name, kind, format!("{}: {}", name.name, self.show(&local.ty)), self.docs.of(name.span));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    pub(crate) fn build_info(&mut self) -> Option<Info> {
        let collected = self.collect.take()?;
        self.docs = Docs::collect(&self.modules);
        let mut info = Info::default();
        let mut types: BTreeMap<String, Ty> = BTreeMap::new();

        for r in collected.exprs {
            let ty = self.resolve(&r.ty);
            if matches!(ty, Ty::Error) {
                continue;
            }
            let shown = r.statics.clone().unwrap_or_else(|| ty.show(&self.defs));
            let label = self.label_of(&r, &ty);
            let doc = self.doc_of(&r);
            if r.statics.is_none() {
                types.entry(shown.clone()).or_insert(ty);
            }
            info.exprs.push(ExprInfo { loc: self.loc(r.span), ty: shown, label, statics: r.statics, doc });
        }
        for l in collected.locals {
            let ty = self.resolve(&l.ty);
            let scope = Span { file: l.scope.file, start: l.span.start, end: l.scope.end };
            info.locals.push(LocalInfo {
                loc: self.loc(l.span),
                name: l.name,
                ty: self.show(&ty),
                kind: match l.kind {
                    LocalKind::Var => "let",
                    LocalKind::Const => "const",
                    LocalKind::Param => "param",
                },
                scope: self.loc(scope),
            });
        }
        info.items = self.definition_sites();
        info.functions = catalog::FUNCTIONS
            .iter()
            .map(|(name, detail, doc)| Member { name: name.to_string(), kind: "function", detail: detail.to_string(), loc: None, doc: Some(doc.to_string()), public: true })
            .collect();
        for (ty_name, ty) in types {
            let members = self.members_of(&ty);
            if !members.is_empty() {
                info.members.insert(ty_name, members);
            }
        }
        for (key, what) in &collected.statics {
            let members = self.statics_of(what);
            info.statics.insert(key.clone(), members);
        }
        for m in 0..self.modules.len() {
            let names = self.item_members(m, false);
            info.globals.insert(self.modules[m].path.clone(), names);
        }
        Some(info)
    }

    /// The documentation comment of the function or type a name refers to.
    fn doc_of(&self, r: &RawExpr) -> Option<String> {
        match self.res.paths.get(&r.id)? {
            PathRes::Fn(fid) => self.defs.fns[*fid].decl.as_ref().and_then(|d| d.name.as_ref()).and_then(|n| self.docs.of(n.span)),
            PathRes::Type(TypeRef::Struct(s)) => self.docs.of(self.defs.structs[*s].span),
            PathRes::Type(TypeRef::Enum(e)) => self.docs.of(self.defs.enums[*e].span),
            PathRes::EnumVariant(e, i) => {
                let span = self.defs.enums[*e].span;
                self.docs.variants.get(&(span.file, span.start)).and_then(|v| v.get(*i)).cloned().flatten()
            }
            PathRes::Global(m, name) => self.mscopes[*m].vars.get(name).and_then(|l| self.docs.of(l.span)),
            _ => None,
        }
    }

    fn label_of(&self, r: &RawExpr, ty: &Ty) -> Option<String> {
        let shown = ty.show(&self.defs);
        match self.res.paths.get(&r.id)? {
            PathRes::Fn(fid) => Some(format!("fun {}", self.fn_signature(*fid, &HashMap::new()))),
            PathRes::Type(TypeRef::Struct(s)) => Some(format!("struct {}", self.defs.structs[*s].name)),
            PathRes::Type(TypeRef::Enum(e)) => Some(format!("enum {}", self.defs.enums[*e].name)),
            PathRes::Type(TypeRef::Builtin(b)) => Some(format!("type {b}")),
            PathRes::Module(m) => Some(format!("module \"{}\"", self.modules[*m].path)),
            PathRes::EnumVariant(e, i) => {
                let def = &self.defs.enums[*e];
                Some(format!("{}.{}", def.name, def.variants[*i]))
            }
            PathRes::Local | PathRes::Global(..) => r.name.as_ref().map(|n| format!("{n}: {shown}")),
            _ => None,
        }
    }
}

fn dedup(items: Vec<Member>) -> Vec<Member> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|m| seen.insert(m.name.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MemoryHost;

    fn info_of(src: &str) -> Info {
        let mut host = MemoryHost::default();
        let options = CompileOptions { info: true };
        match compile_with("main.q", src, &mut host, &options) {
            Ok(p) => p.info.expect("info"),
            Err(e) => *e.info.expect("info even on errors"),
        }
    }

    /// Every method documented in the catalog must exist in the checker, and the other
    /// way round: a new built-in method without documentation fails here.
    #[test]
    fn catalog_matches_the_checker() {
        let info = info_of("let a = [1]\nlet s = \"x\"\nlet m = { \"k\": 1 }\nlet i = 1\nlet f = 1.5\n");
        for (ty, owner) in [("array<int>", "array"), ("string", "string"), ("map<string, int>", "map"), ("int", "int"), ("float", "float")] {
            let documented: Vec<&str> = catalog::methods(owner).iter().map(|e| e.0).collect();
            let known: Vec<&str> = info.members[ty].iter().map(|m| m.name.as_str()).collect();
            assert_eq!(documented, known, "methods of {ty}");
        }
    }

    #[test]
    fn built_in_methods_not_in_the_catalog_are_detected() {
        // every name the checker accepts is documented: probe a list of candidate names
        let mut host = MemoryHost::default();
        for (decl, owner) in [("[1]", "array"), ("\"x\"", "string"), ("{ \"k\": 1 }", "map"), ("1", "int"), ("1.5", "float")] {
            for name in ALL_CANDIDATES {
                let src = format!("let v = {decl}\nlet w = v.{name}\n");
                let accepted = match compile_with("main.q", &src, &mut host, &CompileOptions::default()) {
                    Ok(_) => true,
                    Err(e) => !e.diagnostics.iter().any(|d| d.code == "T161"),
                };
                let documented = catalog::methods(owner).iter().any(|e| e.0 == *name);
                // `v.name` without a call is always an error; "has no field" (T161) means unknown
                assert_eq!(accepted, documented, "{owner}.{name}");
            }
        }
    }

    const ALL_CANDIDATES: &[&str] = &[
        "len", "is_empty", "push", "pop", "insert", "remove", "clear", "contains", "index_of", "reverse", "join", "sort", "min", "max",
        "sum", "slice", "has", "get", "keys", "values", "upper", "lower", "trim", "trim_start", "trim_end", "capitalize", "starts_with",
        "ends_with", "last_index_of", "count", "replace", "split", "repeat", "chars", "lines", "words", "pad_left", "pad_right",
        "is_digit", "is_alpha", "is_blank", "code", "to_int", "to_float", "substring", "char", "abs", "pow", "sqrt", "floor", "ceil",
        "round", "first", "last", "append", "extend", "find", "filter", "map", "length", "size", "swap", "copy", "clone", "to_string",
    ];

    #[test]
    fn types_and_members() {
        let info = info_of("struct P\n  public x: int\nend\nimpl P\n  public fun show(self) -> string\n    return \"p\"\n  end\nend\nlet p = P { x: 1 }\nlet q = p\n");
        assert!(info.exprs.iter().any(|e| e.ty == "P"));
        let names: Vec<&str> = info.members["P"].iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["x", "show"]);
        assert_eq!(info.members["P"][1].detail, "show() -> string");
        assert!(info.locals.iter().any(|l| l.name == "q" && l.ty == "P"));
    }

    #[test]
    fn info_survives_type_errors() {
        let info = info_of("let a = [1, 2]\nlet b: string = a.len()\n");
        assert!(info.exprs.iter().any(|e| e.ty == "array<int>"));
        assert!(info.members.contains_key("array<int>"));
    }
}
