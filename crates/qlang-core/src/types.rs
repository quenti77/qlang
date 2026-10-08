//! Types and definitions shared by the checker and the interpreter.

use crate::ast::{Expr, FunDecl, NodeId, Visibility};
use crate::span::Span;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub type StructId = usize;
pub type EnumId = usize;
pub type TraitId = usize;
pub type FnId = usize;
pub type ImplId = usize;
pub type ParamId = usize;
pub type ModId = usize;

/// The generic parameter standing for "the implementing type" in trait signatures.
pub const SELF_PARAM: ParamId = 0;

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Str,
    /// The type of expressions without a value.
    Unit,
    /// The type of the `none` literal.
    NoneT,
    /// The type of expressions that never finish (`return`, `break`).
    Never,
    Range,
    Nullable(Box<Ty>),
    Array(Box<Ty>),
    Fn(Vec<Ty>, Box<Ty>),
    Struct(StructId, Vec<Ty>),
    Enum(EnumId),
    /// A trait used as a type: any value whose type implements it.
    Trait(TraitId, Vec<Ty>),
    /// A generic parameter.
    Param(ParamId),
    /// A type variable being inferred.
    Infer(u32),
    /// Placeholder after a reported error; compatible with everything.
    Error,
}

impl Ty {
    pub fn nullable(inner: Ty) -> Ty {
        match inner {
            Ty::Nullable(_) | Ty::NoneT | Ty::Error => inner,
            other => Ty::Nullable(Box::new(other)),
        }
    }

    pub fn non_null(&self) -> Ty {
        match self {
            Ty::Nullable(t) => (**t).clone(),
            other => other.clone(),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Replace generic parameters.
    pub fn subst(&self, map: &HashMap<ParamId, Ty>) -> Ty {
        match self {
            Ty::Param(p) => map.get(p).cloned().unwrap_or_else(|| self.clone()),
            Ty::Nullable(t) => Ty::nullable(t.subst(map)),
            Ty::Array(t) => Ty::Array(Box::new(t.subst(map))),
            Ty::Fn(ps, r) => Ty::Fn(ps.iter().map(|p| p.subst(map)).collect(), Box::new(r.subst(map))),
            Ty::Struct(id, args) => Ty::Struct(*id, args.iter().map(|a| a.subst(map)).collect()),
            Ty::Trait(id, args) => Ty::Trait(*id, args.iter().map(|a| a.subst(map)).collect()),
            other => other.clone(),
        }
    }

    pub fn contains_infer(&self) -> bool {
        match self {
            Ty::Infer(_) => true,
            Ty::Nullable(t) | Ty::Array(t) => t.contains_infer(),
            Ty::Fn(ps, r) => ps.iter().any(Ty::contains_infer) || r.contains_infer(),
            Ty::Struct(_, a) | Ty::Trait(_, a) => a.iter().any(Ty::contains_infer),
            _ => false,
        }
    }

    pub fn contains_param(&self) -> bool {
        match self {
            Ty::Param(_) => true,
            Ty::Nullable(t) | Ty::Array(t) => t.contains_param(),
            Ty::Fn(ps, r) => ps.iter().any(Ty::contains_param) || r.contains_param(),
            Ty::Struct(_, a) | Ty::Trait(_, a) => a.iter().any(Ty::contains_param),
            _ => false,
        }
    }

    /// Human-readable form for messages.
    pub fn show(&self, defs: &Defs) -> String {
        match self {
            Ty::Int => "int".into(),
            Ty::Float => "float".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "string".into(),
            Ty::Unit => "no value".into(),
            Ty::NoneT => "none".into(),
            Ty::Never => "never".into(),
            Ty::Range => "range".into(),
            Ty::Nullable(t) => match &**t {
                Ty::Fn(..) => format!("({})?", t.show(defs)),
                _ => format!("{}?", t.show(defs)),
            },
            Ty::Array(t) => format!("array<{}>", t.show(defs)),
            Ty::Fn(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|p| p.show(defs)).collect();
                if **r == Ty::Unit {
                    format!("fun({})", ps.join(", "))
                } else {
                    format!("fun({}) -> {}", ps.join(", "), r.show(defs))
                }
            }
            Ty::Struct(id, args) => named(&defs.structs[*id].name, args, defs),
            Ty::Enum(id) => defs.enums[*id].name.clone(),
            Ty::Trait(id, args) => named(&defs.traits[*id].name, args, defs),
            Ty::Param(p) => defs.params[*p].name.clone(),
            Ty::Infer(_) => "_".into(),
            Ty::Error => "?".into(),
        }
    }
}

fn named(name: &str, args: &[Ty], defs: &Defs) -> String {
    if args.is_empty() {
        name.to_string()
    } else {
        let a: Vec<String> = args.iter().map(|t| t.show(defs)).collect();
        format!("{name}<{}>", a.join(", "))
    }
}

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name: String,
    pub bounds: Vec<(TraitId, Vec<Ty>)>,
}

#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub ty: Ty,
    pub vis: Visibility,
    pub is_static: bool,
    pub default: Option<Rc<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub module: ModId,
    pub span: Span,
    pub tparams: Vec<ParamId>,
    pub parent: Option<(StructId, Vec<Ty>)>,
    /// Fields declared by this struct only.
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: String,
    pub module: ModId,
    pub span: Span,
    pub variants: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SigDef {
    pub name: String,
    pub tparams: Vec<ParamId>,
    pub has_self: bool,
    pub params: Vec<Ty>,
    /// `None`: not constrained by the trait (operator results).
    pub ret: Option<Ty>,
}

/// Traits the language itself knows: they back operators and `as`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Neg,
    Eq,
    Ord,
    Index,
    IndexSet,
    Push,
    As,
}

pub const ALL_LANG: [Lang; 14] = [
    Lang::Add,
    Lang::Sub,
    Lang::Mul,
    Lang::Div,
    Lang::IntDiv,
    Lang::Mod,
    Lang::Pow,
    Lang::Neg,
    Lang::Eq,
    Lang::Ord,
    Lang::Index,
    Lang::IndexSet,
    Lang::Push,
    Lang::As,
];

impl Lang {
    pub fn trait_name(self) -> &'static str {
        match self {
            Lang::Add => "Add",
            Lang::Sub => "Sub",
            Lang::Mul => "Mul",
            Lang::Div => "Div",
            Lang::IntDiv => "IntDiv",
            Lang::Mod => "Mod",
            Lang::Pow => "Pow",
            Lang::Neg => "Neg",
            Lang::Eq => "Eq",
            Lang::Ord => "Ord",
            Lang::Index => "Index",
            Lang::IndexSet => "IndexSet",
            Lang::Push => "Push",
            Lang::As => "As",
        }
    }

    pub fn method_name(self) -> &'static str {
        match self {
            Lang::Add => "add",
            Lang::Sub => "sub",
            Lang::Mul => "mul",
            Lang::Div => "divide",
            Lang::IntDiv => "int_div",
            Lang::Mod => "modulo",
            Lang::Pow => "pow",
            Lang::Neg => "neg",
            Lang::Eq => "eq",
            Lang::Ord => "cmp",
            Lang::Index => "index",
            Lang::IndexSet => "set_index",
            Lang::Push => "push",
            Lang::As => "convert",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TraitDef {
    pub name: String,
    pub module: ModId,
    pub span: Span,
    pub tparams: Vec<ParamId>,
    /// Default argument for each trait parameter (`Rhs = Self`).
    pub defaults: Vec<Option<Ty>>,
    pub sigs: Vec<SigDef>,
    pub lang: Option<Lang>,
}

#[derive(Clone, Debug)]
pub struct FnDef {
    pub name: String,
    pub module: ModId,
    pub span: Span,
    /// Generic parameters in scope: those of the impl, then the function's own.
    pub scope_params: Vec<(String, ParamId)>,
    /// Number of own parameters at the end of `scope_params`.
    pub own_tparams: usize,
    pub impl_id: Option<ImplId>,
    pub has_self: bool,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub vis: Visibility,
    pub is_override: bool,
    pub decl: Option<Rc<FunDecl>>,
}

#[derive(Clone, Debug)]
pub struct ImplDef {
    pub module: ModId,
    pub span: Span,
    pub tparams: Vec<(String, ParamId)>,
    pub trait_: Option<(TraitId, Vec<Ty>)>,
    pub target: Ty,
    pub methods: Vec<(String, FnId)>,
}

/// All definitions of a program.
pub struct Defs {
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
    pub traits: Vec<TraitDef>,
    pub fns: Vec<FnDef>,
    pub impls: Vec<ImplDef>,
    pub params: Vec<ParamDef>,
    lang_ids: Vec<TraitId>,
}

impl Defs {
    pub fn new() -> Defs {
        let mut d = Defs {
            structs: Vec::new(),
            enums: Vec::new(),
            traits: Vec::new(),
            fns: Vec::new(),
            impls: Vec::new(),
            params: Vec::new(),
            lang_ids: Vec::new(),
        };
        let this = d.new_param("Self");
        debug_assert_eq!(this, SELF_PARAM);
        for lang in ALL_LANG {
            let id = d.make_lang_trait(lang);
            d.lang_ids.push(id);
        }
        d
    }

    pub fn new_param(&mut self, name: &str) -> ParamId {
        self.params.push(ParamDef { name: name.to_string(), bounds: Vec::new() });
        self.params.len() - 1
    }

    pub fn lang(&self, l: Lang) -> TraitId {
        self.lang_ids[ALL_LANG.iter().position(|x| *x == l).unwrap()]
    }

    pub fn lang_of(&self, t: TraitId) -> Option<Lang> {
        self.traits[t].lang
    }

    fn make_lang_trait(&mut self, lang: Lang) -> TraitId {
        let this = Ty::Param(SELF_PARAM);
        let (tparams, defaults, params, ret): (Vec<ParamId>, Vec<Option<Ty>>, Vec<Ty>, Option<Ty>) = match lang {
            Lang::Add | Lang::Sub | Lang::Mul | Lang::Div | Lang::IntDiv | Lang::Mod | Lang::Pow => {
                let rhs = self.new_param("Rhs");
                (vec![rhs], vec![Some(this)], vec![Ty::Param(rhs)], None)
            }
            Lang::Neg => (vec![], vec![], vec![], None),
            Lang::Eq => (vec![], vec![], vec![this], Some(Ty::Bool)),
            Lang::Ord => (vec![], vec![], vec![this], Some(Ty::Int)),
            Lang::Index => {
                let i = self.new_param("I");
                (vec![i], vec![None], vec![Ty::Param(i)], None)
            }
            Lang::IndexSet => {
                let i = self.new_param("I");
                let v = self.new_param("V");
                (vec![i, v], vec![None, None], vec![Ty::Param(i), Ty::Param(v)], Some(Ty::Unit))
            }
            Lang::Push => {
                let t = self.new_param("T");
                (vec![t], vec![None], vec![Ty::Param(t)], Some(Ty::Unit))
            }
            Lang::As => {
                let t = self.new_param("T");
                (vec![t], vec![None], vec![], Some(Ty::Param(t)))
            }
        };
        self.traits.push(TraitDef {
            name: lang.trait_name().to_string(),
            module: 0,
            span: Span::default(),
            tparams,
            defaults,
            sigs: vec![SigDef {
                name: lang.method_name().to_string(),
                tparams: vec![],
                has_self: true,
                params,
                ret,
            }],
            lang: Some(lang),
        });
        self.traits.len() - 1
    }

    /// The chain from a struct up to its root ancestor, with type arguments
    /// substituted: `[(Savings, args), (Account, parent_args), ...]`.
    pub fn ancestry(&self, id: StructId, args: &[Ty]) -> Vec<(StructId, Vec<Ty>)> {
        let mut out = vec![(id, args.to_vec())];
        let mut cur = (id, args.to_vec());
        let mut guard = 0;
        while let Some((pid, pargs)) = self.structs[cur.0].parent.clone() {
            let map: HashMap<ParamId, Ty> = self.structs[cur.0]
                .tparams
                .iter()
                .cloned()
                .zip(cur.1.iter().cloned())
                .collect();
            let next = (pid, pargs.iter().map(|t| t.subst(&map)).collect::<Vec<_>>());
            out.push(next.clone());
            cur = next;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        out
    }

    pub fn is_subclass(&self, child: StructId, ancestor: StructId) -> bool {
        self.ancestry(child, &[]).iter().any(|(s, _)| *s == ancestor)
    }
}

impl Default for Defs {
    fn default() -> Self {
        Defs::new()
    }
}

// ------------------------------------------------------------- resolutions

#[derive(Clone, Debug, PartialEq)]
pub enum TypeRef {
    Struct(StructId),
    Enum(EnumId),
    /// `int`, `float`, `bool`, `string`, `array`
    Builtin(&'static str),
}

/// What a name or path in the source refers to, recorded by the checker
/// for the interpreter.
#[derive(Clone, Debug, PartialEq)]
pub enum PathRes {
    Local,
    Global(ModId, String),
    Fn(FnId),
    Type(TypeRef),
    Module(ModId),
    EnumVariant(EnumId, usize),
    StaticField(StructId, String),
    BuiltinFn(&'static str),
    /// `int.parse`
    BuiltinStatic(&'static str, &'static str),
}

#[derive(Default)]
pub struct Resolutions {
    pub paths: HashMap<NodeId, PathRes>,
    /// Target type of each `as` cast.
    pub casts: HashMap<NodeId, Ty>,
    pub struct_lits: HashMap<NodeId, StructId>,
    /// `super.method` calls: the parent method being called.
    pub super_calls: HashMap<NodeId, FnId>,
    /// Enum variant patterns: (match expression, case index) to variant.
    pub patterns: HashMap<(NodeId, usize), (EnumId, usize)>,
    /// `xs.sum()` calls on `array<float>` (an empty sum is `0.0`, not `0`).
    pub float_sums: HashSet<NodeId>,
}
