//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod body;
mod calls;
mod catalog;
mod info;
mod decls;
mod expr;
mod unify;

pub use info::{ExprInfo, Info, ItemInfo, LocalInfo, Member};

use crate::ast::{self, Item};
use crate::diag::{Diagnostic, MAX_DIAGNOSTICS, Severity};
use crate::host::{Capabilities, Host};
use crate::parser::parse_module_from;
use crate::span::{SourceMap, Span, resolve_module_path};
use crate::types::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct ModuleData {
    pub path: String,
    pub file: u32,
    pub ast: Rc<ast::Module>,
}

/// A checked program: sources, definitions and what each name refers to.
pub struct Program {
    pub sources: SourceMap,
    /// In execution order: a module comes after the modules it imports.
    pub modules: Vec<ModuleData>,
    pub defs: Defs,
    pub res: Resolutions,
    pub warnings: Vec<Diagnostic>,
    /// For editors; only when asked for (see [`CompileOptions`]).
    pub info: Option<Info>,
}

/// Compilation failed. Diagnostics carry resolved locations.
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
    /// What the checker learned before stopping (only when asked for).
    pub info: Option<Box<Info>>,
}

impl CompileError {
    pub fn render(&self) -> String {
        self.diagnostics.iter().map(|d| d.render(&self.sources)).collect::<Vec<_>>().join("\n")
    }
}

/// Compile the program whose entry file is `entry` with text `source`.
/// Imported modules are requested from `host`.
pub fn compile(entry: &str, source: &str, host: &mut dyn Host) -> Result<Program, CompileError> {
    compile_with(entry, source, host, &CompileOptions::default())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompileOptions {
    /// Also collect the types of expressions, local variables and members (for editors).
    pub info: bool,
}

/// Like [`compile`], with options.
pub fn compile_with(entry: &str, source: &str, host: &mut dyn Host, options: &CompileOptions) -> Result<Program, CompileError> {
    // A bug in the compiler must not take the caller down with it (a server, an editor).
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_inner(entry, source, host, options))) {
        Ok(result) => result,
        Err(payload) => Err(CompileError {
            diagnostics: vec![internal_error(&payload)],
            sources: SourceMap::default(),
            info: None,
        }),
    }
}

/// A diagnostic for a panic caught inside qlang itself.
pub(crate) fn internal_error(payload: &Box<dyn std::any::Any + Send>) -> Diagnostic {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".to_string());
    Diagnostic::error("X999", format!("internal error: {detail} (this is a bug in qlang, please report it)"), Span::default())
}

fn compile_inner(entry: &str, source: &str, host: &mut dyn Host, options: &CompileOptions) -> Result<Program, CompileError> {
    let caps = host.capabilities();
    let mut loader = Loader {
        host,
        sources: SourceMap::default(),
        diags: Vec::new(),
        modules: Vec::new(),
        done: HashMap::new(),
        loading: Vec::new(),
        next_id: 0,
    };
    let entry_key = loader.host.canonical_path(entry);
    loader.load(entry, entry_key, source.to_string());
    let Loader { sources, mut diags, modules, .. } = loader;

    let has_errors = |d: &[Diagnostic]| d.iter().any(|x| x.severity == Severity::Error);
    if has_errors(&diags) {
        return Err(finish_error(diags, sources, None));
    }

    let mut ck = Checker::new(sources, modules, caps);
    if options.info {
        ck.collect = Some(info::Collector::default());
    }
    ck.run();
    diags.append(&mut ck.diags);
    let info = if options.info { ck.build_info() } else { None };
    if has_errors(&diags) {
        return Err(finish_error(diags, ck.sources, info));
    }
    for d in &mut diags {
        d.resolve(&ck.sources);
    }
    Ok(Program {
        sources: ck.sources,
        modules: ck
            .modules
            .into_iter()
            .map(|m| ModuleData { path: m.path, file: m.file, ast: m.ast })
            .collect(),
        defs: ck.defs,
        res: ck.res,
        warnings: diags,
        info,
    })
}

fn finish_error(mut diags: Vec<Diagnostic>, sources: SourceMap, info: Option<Info>) -> CompileError {
    // stable order: by file then position
    diags.sort_by_key(|d| (d.span.file, d.span.start));
    if diags.len() > MAX_DIAGNOSTICS {
        let last = diags[MAX_DIAGNOSTICS - 1].span;
        diags.truncate(MAX_DIAGNOSTICS);
        diags.push(Diagnostic::error(
            "T000",
            format!("too many errors: only the first {MAX_DIAGNOSTICS} are shown"),
            last,
        ));
    }
    for d in &mut diags {
        d.resolve(&sources);
    }
    CompileError { diagnostics: diags, sources, info: info.map(Box::new) }
}

// ------------------------------------------------------------------ loading

pub(crate) struct Loaded {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    /// Item index of each `import` to the index of the module it loads.
    imports: HashMap<usize, usize>,
}

struct Loader<'h> {
    host: &'h mut dyn Host,
    sources: SourceMap,
    diags: Vec<Diagnostic>,
    modules: Vec<Loaded>,
    done: HashMap<String, usize>,
    loading: Vec<String>,
    /// Node ids are unique across all modules.
    next_id: u32,
}

impl Loader<'_> {
    /// `key` identifies the module (see `Host::canonical_path`); `path` is how it is named.
    fn load(&mut self, path: &str, key: String, text: String) -> usize {
        let file = self.sources.add(path, &text);
        self.loading.push(key.clone());
        let (ast, next_id) = parse_module_from(&text, file, self.next_id, &mut self.diags);
        self.next_id = next_id;
        let mut imports = HashMap::new();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(target) = resolve_module_path(path, &imp.path) else {
                crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T100",
                    format!(
                        "cannot import \"{}\": use a path relative to this file, written with `/` (`./x.q`, `../lib/x.q`), not an absolute one",
                        imp.path
                    ),
                    imp.path_span,
                ));
                continue;
            };
            let target_key = self.host.canonical_path(&target);
            if self.loading.contains(&target_key) {
                crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T101",
                    format!("circular import of \"{}\"", imp.path),
                    imp.path_span,
                ));
                continue;
            }
            if let Some(&m) = self.done.get(&target_key) {
                imports.insert(idx, m);
                continue;
            }
            match self.host.load_module(&target) {
                Ok(src) => {
                    let m = self.load(&target, target_key, src);
                    imports.insert(idx, m);
                }
                Err(e) => crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T102",
                    format!("cannot import \"{}\": {e}", imp.path),
                    imp.path_span,
                )),
            }
        }
        self.loading.pop();
        let idx = self.modules.len();
        self.modules.push(Loaded { path: path.to_string(), file, ast: Rc::new(ast), imports });
        self.done.insert(key, idx);
        idx
    }
}

// ------------------------------------------------------------------ checker

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ItemRef {
    Fn(FnId),
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
    Var(ModId, String),
    Module(ModId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalKind {
    Var,
    Const,
    Param,
}

#[derive(Clone, Debug)]
pub(crate) struct Local {
    pub ty: Ty,
    pub kind: LocalKind,
    /// Set when `ty` is a narrowed view (`x != none`) of this declared type.
    pub orig: Option<Ty>,
    pub span: Span,
}

#[derive(Default)]
pub(crate) struct ModScope {
    pub items: HashMap<String, ItemRef>,
    pub exports: HashSet<String>,
    /// Top-level variables, by name.
    pub vars: HashMap<String, Local>,
    /// Item index in the file to the id of its definition.
    pub decl_ids: HashMap<usize, usize>,
}

pub(crate) struct Checker {
    pub caps: Capabilities,
    pub sources: SourceMap,
    pub diags: Vec<Diagnostic>,
    pub defs: Defs,
    pub res: Resolutions,
    pub modules: Vec<Loaded>,
    pub mscopes: Vec<ModScope>,

    // --- state of the body being checked
    pub cur_mod: ModId,
    pub scopes: Vec<HashMap<String, Local>>,
    pub tparam_scope: Vec<(String, ParamId)>,
    /// Return type of the enclosing function or lambda.
    pub ret_stack: Vec<Ty>,
    pub loop_depth: usize,
    /// The struct whose `impl` is being checked (for visibility).
    pub cur_impl_struct: Option<StructId>,
    /// The type of `self` in the current method.
    pub cur_self: Option<Ty>,
    /// The method being checked (for `super`).
    pub cur_fn: Option<FnId>,
    pub subst: Vec<Option<Ty>>,
    /// Set when an editor asked for [`Info`].
    pub collect: Option<info::Collector>,
}

impl Checker {
    fn new(sources: SourceMap, modules: Vec<Loaded>, caps: Capabilities) -> Checker {
        let n = modules.len();
        Checker {
            caps,
            sources,
            diags: Vec::new(),
            defs: Defs::new(),
            res: Resolutions::default(),
            modules,
            mscopes: (0..n).map(|_| ModScope::default()).collect(),
            cur_mod: 0,
            scopes: Vec::new(),
            tparam_scope: Vec::new(),
            ret_stack: Vec::new(),
            loop_depth: 0,
            cur_impl_struct: None,
            cur_self: None,
            cur_fn: None,
            subst: Vec::new(),
            collect: None,
        }
    }

    pub fn err(&mut self, code: &str, msg: impl Into<String>, span: Span) {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span));
    }

    pub fn err_note(&mut self, code: &str, msg: impl Into<String>, span: Span, note: impl Into<String>, note_span: Option<Span>) {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span).with_note(note, note_span));
    }

    pub fn show(&self, t: &Ty) -> String {
        let t = self.resolve(t);
        t.show(&self.defs)
    }

    fn run(&mut self) {
        let n = self.modules.len();
        // A: declare names
        for m in 0..n {
            self.cur_mod = m;
            self.declare_module(m);
        }
        // B: resolve signatures
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_traits(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_structs(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_functions(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_impls(m);
        }
        self.validate_structs();
        self.validate_overrides();
        if self.diags.iter().any(|d| d.severity == Severity::Error) {
            return;
        }
        // C: bodies
        for m in 0..n {
            self.cur_mod = m;
            self.check_module_body(m);
        }
    }
}
