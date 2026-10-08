//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod body;
mod calls;
mod decls;
mod expr;
mod unify;

use crate::ast::{self, Item};
use crate::diag::{Diagnostic, Severity};
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
}

/// Compilation failed. Diagnostics carry resolved locations.
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
}

impl CompileError {
    pub fn render(&self) -> String {
        self.diagnostics.iter().map(|d| d.render(&self.sources)).collect::<Vec<_>>().join("\n")
    }
}

/// Compile the program whose entry file is `entry` with text `source`.
/// Imported modules are requested from `host`.
pub fn compile(entry: &str, source: &str, host: &mut dyn Host) -> Result<Program, CompileError> {
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
    loader.load(entry, source.to_string());
    let Loader { sources, mut diags, modules, .. } = loader;

    let has_errors = |d: &[Diagnostic]| d.iter().any(|x| x.severity == Severity::Error);
    if has_errors(&diags) {
        return Err(finish_error(diags, sources));
    }

    let mut ck = Checker::new(sources, modules, caps);
    ck.run();
    diags.append(&mut ck.diags);
    if has_errors(&diags) {
        return Err(finish_error(diags, ck.sources));
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
    })
}

fn finish_error(mut diags: Vec<Diagnostic>, sources: SourceMap) -> CompileError {
    for d in &mut diags {
        d.resolve(&sources);
    }
    // stable order: by file then position
    diags.sort_by_key(|d| (d.span.file, d.span.start));
    CompileError { diagnostics: diags, sources }
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
    fn load(&mut self, path: &str, text: String) -> usize {
        let file = self.sources.add(path, &text);
        self.loading.push(path.to_string());
        let (ast, next_id) = parse_module_from(&text, file, self.next_id, &mut self.diags);
        self.next_id = next_id;
        let mut imports = HashMap::new();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(target) = resolve_module_path(path, &imp.path) else {
                self.diags.push(Diagnostic::error(
                    "T100",
                    format!("cannot import \"{}\": the path must be relative and stay inside the project", imp.path),
                    imp.path_span,
                ));
                continue;
            };
            if self.loading.contains(&target) {
                self.diags.push(Diagnostic::error(
                    "T101",
                    format!("circular import of \"{}\"", imp.path),
                    imp.path_span,
                ));
                continue;
            }
            if let Some(&m) = self.done.get(&target) {
                imports.insert(idx, m);
                continue;
            }
            match self.host.load_module(&target) {
                Ok(src) => {
                    let m = self.load(&target, src);
                    imports.insert(idx, m);
                }
                Err(e) => self.diags.push(Diagnostic::error(
                    "T102",
                    format!("cannot import \"{}\": {e}", imp.path),
                    imp.path_span,
                )),
            }
        }
        self.loading.pop();
        let idx = self.modules.len();
        self.modules.push(Loaded { path: path.to_string(), file, ast: Rc::new(ast), imports });
        self.done.insert(path.to_string(), idx);
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
        }
    }

    pub fn err(&mut self, code: &str, msg: impl Into<String>, span: Span) {
        self.diags.push(Diagnostic::error(code, msg, span));
    }

    pub fn err_note(&mut self, code: &str, msg: impl Into<String>, span: Span, note: impl Into<String>, note_span: Option<Span>) {
        self.diags.push(Diagnostic::error(code, msg, span).with_note(note, note_span));
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
