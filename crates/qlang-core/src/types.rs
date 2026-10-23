//! Types and definitions shared by the checker and the interpreter.

use crate::ast::{Expr, FunDecl, NodeId, Visibility};
use crate::fx::{FxHashMap, FxHashSet, NodeMap};
use crate::span::Span;
use std::collections::HashMap;
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
    /// A 128-bit universally unique identifier.
    Uuid,
    /// A 128-bit identifier sortable by creation time.
    Ulid,
    /// An instant, in UTC.
    DateTime,
    /// The type of expressions without a value.
    Unit,
    /// The type of the `none` literal.
    NoneT,
    /// The type of expressions that never finish (`return`, `break`).
    Never,
    Range,
    Nullable(Box<Ty>),
    Array(Box<Ty>),
    /// `map<K, V>`: keys are int, string, bool or enum values.
    Map(Box<Ty>, Box<Ty>),
    /// `(int, string)`: two or more types, a fixed number of values of different types.
    Tuple(Vec<Ty>),
    Fn(Vec<Ty>, Box<Ty>),
    Struct(StructId, Vec<Ty>),
    /// An enumeration, with the type arguments of a generic one (`Result<int, Error>`).
    Enum(EnumId, Vec<Ty>),
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
            Ty::Map(k, v) => Ty::Map(Box::new(k.subst(map)), Box::new(v.subst(map))),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|t| t.subst(map)).collect()),
            Ty::Fn(ps, r) => Ty::Fn(
                ps.iter().map(|p| p.subst(map)).collect(),
                Box::new(r.subst(map)),
            ),
            Ty::Struct(id, args) => Ty::Struct(*id, args.iter().map(|a| a.subst(map)).collect()),
            Ty::Enum(id, args) => Ty::Enum(*id, args.iter().map(|a| a.subst(map)).collect()),
            Ty::Trait(id, args) => Ty::Trait(*id, args.iter().map(|a| a.subst(map)).collect()),
            other => other.clone(),
        }
    }

    pub fn contains_infer(&self) -> bool {
        match self {
            Ty::Infer(_) => true,
            Ty::Nullable(t) | Ty::Array(t) => t.contains_infer(),
            Ty::Map(k, v) => k.contains_infer() || v.contains_infer(),
            Ty::Tuple(ts) => ts.iter().any(Ty::contains_infer),
            Ty::Fn(ps, r) => ps.iter().any(Ty::contains_infer) || r.contains_infer(),
            Ty::Struct(_, a) | Ty::Enum(_, a) | Ty::Trait(_, a) => a.iter().any(Ty::contains_infer),
            _ => false,
        }
    }

    pub fn contains_param(&self) -> bool {
        match self {
            Ty::Param(_) => true,
            Ty::Nullable(t) | Ty::Array(t) => t.contains_param(),
            Ty::Map(k, v) => k.contains_param() || v.contains_param(),
            Ty::Tuple(ts) => ts.iter().any(Ty::contains_param),
            Ty::Fn(ps, r) => ps.iter().any(Ty::contains_param) || r.contains_param(),
            Ty::Struct(_, a) | Ty::Enum(_, a) | Ty::Trait(_, a) => a.iter().any(Ty::contains_param),
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
            Ty::Uuid => "uuid".into(),
            Ty::Ulid => "ulid".into(),
            Ty::DateTime => "datetime".into(),
            Ty::Unit => "no value".into(),
            Ty::NoneT => "none".into(),
            Ty::Never => "never".into(),
            Ty::Range => "range".into(),
            Ty::Nullable(t) => match &**t {
                Ty::Fn(..) => format!("({})?", t.show(defs)),
                _ => format!("{}?", t.show(defs)),
            },
            Ty::Array(t) => format!("array<{}>", t.show(defs)),
            Ty::Map(k, v) => format!("map<{}, {}>", k.show(defs), v.show(defs)),
            Ty::Tuple(ts) => {
                let ts: Vec<String> = ts.iter().map(|t| t.show(defs)).collect();
                format!("({})", ts.join(", "))
            }
            Ty::Fn(ps, r) => {
                let ps: Vec<String> = ps.iter().map(|p| p.show(defs)).collect();
                if **r == Ty::Unit {
                    format!("fun({})", ps.join(", "))
                } else {
                    format!("fun({}) -> {}", ps.join(", "), r.show(defs))
                }
            }
            Ty::Struct(id, args) => named(&defs.structs[*id].name, args, defs),
            Ty::Enum(id, args) => named(&defs.enums[*id].name, args, defs),
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
    /// Declared with `#[annotation]`: it can be used as an annotation.
    pub annotation: bool,
    /// Fields declared by this struct only.
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: String,
    pub module: ModId,
    pub span: Span,
    pub tparams: Vec<ParamId>,
    pub variants: Vec<VariantDef>,
}

/// A variant and the data it carries (`Circle(radius: float)`); none for `Empty`.
#[derive(Clone, Debug)]
pub struct VariantDef {
    pub name: String,
    pub span: Span,
    pub fields: Vec<(String, Ty)>,
}

impl EnumDef {
    pub fn variant_index(&self, name: &str) -> Option<usize> {
        self.variants.iter().position(|v| v.name == name)
    }

    /// No variant carries data: the values are plain names (usable as map keys).
    pub fn is_plain(&self) -> bool {
        self.variants.iter().all(|v| v.fields.is_empty())
    }
}

#[derive(Clone, Debug)]
pub struct SigDef {
    pub name: String,
    pub tparams: Vec<ParamId>,
    pub has_self: bool,
    pub params: Vec<Ty>,
    /// `None`: not constrained by the trait (operator results).
    pub ret: Option<Ty>,
    /// The method's default body, if the trait declared one: an impl that
    /// doesn't define this method uses it instead of failing to compile.
    pub default: Option<FnId>,
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
    /// Set for a trait's default method body: the type to give `self` while
    /// checking it (the trait itself, applied to its own type parameters).
    /// `impl_id` is `None` for these: they belong to no concrete impl.
    pub trait_default_self: Option<Ty>,
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
        self.params.push(ParamDef {
            name: name.to_string(),
            bounds: Vec::new(),
        });
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
        let (tparams, defaults, params, ret): (Vec<ParamId>, Vec<Option<Ty>>, Vec<Ty>, Option<Ty>) =
            match lang {
                Lang::Add
                | Lang::Sub
                | Lang::Mul
                | Lang::Div
                | Lang::IntDiv
                | Lang::Mod
                | Lang::Pow => {
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
                    (
                        vec![i, v],
                        vec![None, None],
                        vec![Ty::Param(i), Ty::Param(v)],
                        Some(Ty::Unit),
                    )
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
                default: None,
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
        self.ancestry(child, &[])
            .iter()
            .any(|(s, _)| *s == ancestor)
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
    /// `Point.class`: the name of a struct or enum, known at compile time.
    ClassName(String),
    /// A call of `Reflect`: described by `ReflectOp`.
    Reflect(ReflectOp),
}

/// What a call of `Reflect` asks for (see `docs/language.md`, « Réflexion »).
#[derive(Clone, Debug, PartialEq)]
pub enum ReflectOp {
    /// `Reflect.struct_info<T>()`
    Struct(StructId),
    /// `Reflect.enum_info<T>()`
    Enum(EnumId),
    /// `Reflect.functions()` in this module.
    Functions(ModId),
    /// `Reflect.find<F>(name)`: a function of this module whose type is `F`.
    Find(ModId, Ty),
    /// `Reflect.methods<T>()`: the methods of the impls of the type `T`.
    Methods(Ty),
    /// `Reflect.find_method<T, F>(value, name)`: a method of `T`, bound to `value`, of type `F`.
    FindMethod(Ty, Ty),
}

/// A declaration an annotation is put on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnKey {
    Struct(StructId),
    Field(StructId, usize),
    Enum(EnumId),
    Variant(EnumId, usize),
    Fn(FnId),
    Param(FnId, usize),
}

/// The value of an annotation argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    None,
    Array(Vec<Lit>),
}

/// An annotation as reflection reads it: the name of its struct and its arguments by field.
#[derive(Clone, Debug)]
pub struct AnnData {
    pub name: String,
    pub args: Vec<(String, Lit)>,
}

#[derive(Default)]
pub struct Resolutions {
    pub paths: NodeMap<PathRes>,
    /// Target type of each `as` cast.
    pub casts: FxHashMap<NodeId, Ty>,
    pub struct_lits: FxHashMap<NodeId, StructId>,
    /// `super.method` calls: the parent method being called.
    pub super_calls: FxHashMap<NodeId, FnId>,
    /// Enum variant patterns (`Pattern::Path`, `Variant`, and a `Bind` that names a variant):
    /// the pattern's id to the variant.
    pub patterns: FxHashMap<NodeId, (EnumId, usize)>,
    /// The type of each use of a variable whose type is `int`, `float` or `bool` (the typed
    /// register machine needs it for the variables a loop shares with the code around it).
    pub ident_tys: FxHashMap<NodeId, Ty>,
    /// For a use of a `T?` variable that a test narrowed to `T`: the narrowed type (the type
    /// in `ident_tys` is then the declared one).
    pub ident_narrow: FxHashMap<NodeId, Ty>,
    /// The annotations of each declaration, for `Reflect`.
    pub annotations: FxHashMap<AnnKey, Vec<AnnData>>,
    /// The type of each array and map literal.
    pub array_lits: FxHashMap<NodeId, Ty>,
    /// The type of the variable a `let` declares (keyed by the initializer), when it is a type
    /// the typed machine cares about: `T?`.
    pub let_tys: FxHashMap<NodeId, Ty>,
    /// The type of an expression whose type the typed machine cannot recompute: a call of a
    /// generic function (`Ok(1)`) and a variant of an enumeration. Resolved when the checking ends.
    pub expr_tys: FxHashMap<NodeId, Ty>,
    /// The signature of each call of a built-in method (keyed by the call): receiver,
    /// parameters and result, once the types are known.
    pub builtin_sigs: FxHashMap<NodeId, (Ty, Vec<Ty>, Ty)>,
    /// `xs.sum()` calls on `array<float>` (an empty sum is `0.0`, not `0`).
    pub float_sums: FxHashSet<NodeId>,
    /// Method calls made through a trait (a generic bound or a trait-typed value): at
    /// run time they go to the user's impl, not to a built-in method of the same name.
    pub trait_calls: FxHashSet<NodeId>,
    /// `value.class()`: the concrete struct or enum name of the value at run time
    /// (works even when `value`'s static type is a trait or a generic parameter).
    pub class_calls: FxHashSet<NodeId>,
    /// `expr?` that applies to a `Result` (the others apply to a nullable value).
    pub result_tries: FxHashSet<NodeId>,
    /// Calls with at least one named argument (`f(a: 1)`): for each argument, in the
    /// order written, which parameter slot it fills. Absent means purely positional
    /// (the common case), so the interpreter binds arguments in writing order as-is.
    pub arg_order: FxHashMap<NodeId, Vec<usize>>,
}
