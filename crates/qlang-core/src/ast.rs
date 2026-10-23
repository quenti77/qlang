//! Syntax tree produced by the parser.

use crate::span::Span;
use std::rc::Rc;

pub type NodeId = u32;

#[derive(Debug)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Debug)]
// the tree is built once per program: the size of one variant does not matter
#[allow(clippy::large_enum_variant)]
pub enum Item {
    Import(Import),
    Export(Export),
    Fun(Rc<FunDecl>),
    Struct(StructDecl),
    Impl(ImplDecl),
    Trait(TraitDecl),
    Enum(EnumDecl),
    /// `type Handler = fun(Request) -> Response`
    Alias(AliasDecl),
    Stmt(Stmt),
}

#[derive(Debug)]
pub struct AliasDecl {
    pub span: Span,
    pub name: Ident,
    pub ty: Rc<TypeExpr>,
}

#[derive(Debug)]
pub struct Import {
    pub span: Span,
    pub path: String,
    pub path_span: Span,
    pub kind: ImportKind,
}

#[derive(Debug)]
pub enum ImportKind {
    /// `import a, b as c from "x.q"`
    Names(Vec<ImportName>),
    /// `import "x.q" as m`
    Module(Ident),
}

#[derive(Debug)]
pub struct ImportName {
    pub name: Ident,
    pub alias: Option<Ident>,
}

#[derive(Debug)]
pub struct Export {
    pub span: Span,
    pub names: Vec<Ident>,
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct TypeParam {
    pub name: Ident,
    pub bounds: Vec<TypeExpr>,
}

/// `#[Route("/users", method: "GET")]`: a note on a declaration, read by the checker and
/// (through reflection) by the program.
#[derive(Debug)]
pub struct Annotation {
    pub span: Span,
    /// The name of the annotation struct: `Route` or `m.Route`.
    pub path: Vec<Ident>,
    pub args: Vec<CallArg>,
}

#[derive(Debug)]
pub struct Param {
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub ty: TypeExpr,
}

#[derive(Debug)]
pub struct FunDecl {
    pub annotations: Vec<Annotation>,
    /// The documentation comment (`--" ... --"`) right above it.
    pub doc: Option<Rc<str>>,
    pub id: NodeId,
    pub span: Span,
    pub name: Option<Ident>,
    pub tparams: Vec<TypeParam>,
    /// `self` appeared as the first parameter.
    pub has_self: bool,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    /// `None` for trait signatures and for `extern fun`.
    pub body: Option<Block>,
    /// `extern fun`: provided by the host environment, no body.
    pub external: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug)]
pub struct FieldDecl {
    pub annotations: Vec<Annotation>,
    pub doc: Option<Rc<str>>,
    pub vis: Option<Visibility>,
    pub is_static: bool,
    pub name: Ident,
    pub ty: TypeExpr,
    pub default: Option<Rc<Expr>>,
}

#[derive(Debug)]
pub struct StructDecl {
    pub annotations: Vec<Annotation>,
    pub doc: Option<Rc<str>>,
    pub span: Span,
    pub name: Ident,
    pub tparams: Vec<TypeParam>,
    pub parent: Option<TypeExpr>,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug)]
pub struct MethodDecl {
    pub vis: Option<Visibility>,
    pub is_override: bool,
    pub fun: Rc<FunDecl>,
}

#[derive(Debug)]
pub struct ImplDecl {
    pub span: Span,
    pub tparams: Vec<TypeParam>,
    /// For `impl Trait for Type`, the trait; otherwise `None`.
    pub trait_ref: Option<TypeExpr>,
    pub target: TypeExpr,
    pub methods: Vec<MethodDecl>,
}

#[derive(Debug)]
pub struct TraitDecl {
    pub annotations: Vec<Annotation>,
    pub doc: Option<Rc<str>>,
    pub span: Span,
    pub name: Ident,
    pub tparams: Vec<TypeParam>,
    pub sigs: Vec<Rc<FunDecl>>,
}

#[derive(Debug)]
pub struct EnumDecl {
    pub annotations: Vec<Annotation>,
    pub doc: Option<Rc<str>>,
    pub span: Span,
    pub name: Ident,
    pub tparams: Vec<TypeParam>,
    pub variants: Vec<VariantDecl>,
}

/// `Circle(radius: float)` or `Empty`.
#[derive(Debug)]
pub struct VariantDecl {
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    /// The documentation comment right above it.
    pub doc: Option<Rc<str>>,
    pub fields: Vec<Param>,
}

#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub span: Span,
    pub kind: TypeExprKind,
}

#[derive(Clone, Debug)]
pub enum TypeExprKind {
    /// `a.b.C<args>`
    Named {
        path: Vec<Ident>,
        args: Vec<TypeExpr>,
    },
    Nullable(Box<TypeExpr>),
    /// `(int, string)`: two or more types.
    Tuple(Vec<TypeExpr>),
    Fun {
        params: Vec<TypeExpr>,
        ret: Option<Box<TypeExpr>>,
    },
}

#[derive(Debug)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

#[derive(Debug)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

/// What `let (a, (b, _)) = t` and `for (a, b) in pairs` take a tuple apart with.
#[derive(Debug)]
pub enum TuplePattern {
    /// A variable that receives the element.
    Name(Ident),
    /// `_`: the element is ignored.
    Wild(Span),
    /// `(a, b)`: the element is itself a tuple.
    Tuple(Vec<TuplePattern>, Span),
}

impl TuplePattern {
    /// The variables the pattern declares, in order.
    pub fn names<'a>(&'a self, out: &mut Vec<&'a Ident>) {
        match self {
            TuplePattern::Name(n) => out.push(n),
            TuplePattern::Wild(_) => {}
            TuplePattern::Tuple(items, _) => items.iter().for_each(|i| i.names(out)),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            TuplePattern::Name(n) => n.span,
            TuplePattern::Wild(s) | TuplePattern::Tuple(_, s) => *s,
        }
    }
}

#[derive(Debug)]
pub enum StmtKind {
    Let {
        /// The documentation comment right above it.
        doc: Option<Rc<str>>,
        is_const: bool,
        name: Ident,
        ty: Option<TypeExpr>,
        init: Expr,
    },
    /// `let (a, b) = expr`
    LetTuple {
        is_const: bool,
        pattern: TuplePattern,
        ty: Option<TypeExpr>,
        init: Expr,
    },
    /// Statements run one after the other in the enclosing scope. The parser produces it for
    /// `(a, b) = (b, a)`: a hidden variable takes the value, then each target is assigned its
    /// element. `arity` is the number of targets of that tuple assignment, to be checked.
    Seq {
        stmts: Vec<Stmt>,
        arity: Option<(usize, Span)>,
    },
    Return(Option<Expr>),
    While {
        cond: Expr,
        body: Block,
    },
    For {
        var: Ident,
        /// `for key, value in map` / `for index, item in array`
        var2: Option<Ident>,
        /// `for (a, b) in pairs`: `var` is then empty and these take the value apart.
        var_pattern: Option<TuplePattern>,
        var2_pattern: Option<TuplePattern>,
        iter: Expr,
        step: Option<Expr>,
        body: Block,
    },
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug)]
pub struct Expr {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::IntDiv => "div",
            BinOp::Mod => "mod",
            BinOp::Pow => "**",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug)]
pub enum StrPart {
    Lit(String),
    Expr(Expr),
}

#[derive(Debug)]
pub struct StructLitItem {
    pub name: Ident,
    pub value: Expr,
}

/// An argument in a call: `f(1, b: 2)`. `name` is `Some` for a named argument.
#[derive(Debug)]
pub struct CallArg {
    pub name: Option<Ident>,
    pub value: Expr,
}

#[derive(Debug)]
pub struct StructLit {
    /// The path of the type being built: `Point` or `m.Point`.
    pub path: Vec<Ident>,
    pub targs: Vec<TypeExpr>,
    pub base: Option<Box<Expr>>,
    pub fields: Vec<StructLitItem>,
}

#[derive(Debug)]
pub enum Pattern {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    /// `1..5`, `1..=5`
    Range {
        lo: Box<Pattern>,
        hi: Box<Pattern>,
        inclusive: bool,
    },
    /// `Color.Red`, `m.Color.Red`; `id` keys what the checker resolved it to.
    Path {
        path: Vec<Ident>,
        id: NodeId,
    },
    /// `x`: matches anything and binds the value (or, when `x` is a variant of the type
    /// matched, that variant: the checker decides, by `id`).
    Bind {
        name: Ident,
        id: NodeId,
    },
    /// `(0, y)`, `(a, b)`: a tuple whose elements match the patterns, in order.
    Tuple(Vec<Pattern>),
    /// `Circle(r)`, `Shape.Circle(r, _)`, `Ok((a, b))`: a variant that carries data; each
    /// field is matched by a pattern (a name, `_`, a value, a tuple, another variant…).
    Variant {
        path: Vec<Ident>,
        fields: Vec<Pattern>,
        id: NodeId,
    },
}

impl Pattern {
    /// The variables the pattern declares.
    pub fn bound_names<'a>(&'a self, out: &mut Vec<&'a Ident>) {
        match self {
            Pattern::Bind { name, .. } => out.push(name),
            Pattern::Variant { fields, .. } => fields.iter().for_each(|p| p.bound_names(out)),
            Pattern::Tuple(items) => items.iter().for_each(|p| p.bound_names(out)),
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct MatchCase {
    pub span: Span,
    pub pattern: Pattern,
    pub pattern_span: Span,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Debug)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(Vec<StrPart>),
    Bool(bool),
    None,
    Ident(String, Vec<TypeExpr>),
    SelfVal,
    Super,
    Array(Vec<Expr>),
    /// `{ key: value, ... }`
    Map(Vec<(Expr, Expr)>),
    StructLit(Box<StructLit>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `target = value`, or `target += value` with `op = Some(Add)`.
    Assign {
        op: Option<BinOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Cast(Box<Expr>, TypeExpr),
    /// `expr?`: the value of a `Result` or nullable, or leave the function with the error / `none`.
    Try(Box<Expr>),
    /// `(a, b)`: two or more values.
    Tuple(Vec<Expr>),
    /// `t.0`: the element of a tuple at this position.
    TupleField(Box<Expr>, usize),
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    Index(Box<Expr>, Box<Expr>),
    /// `target[]`: only valid as the target of `=` (appends a value).
    Append(Box<Expr>),
    Field(Box<Expr>, Ident, Vec<TypeExpr>),
    If {
        branches: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },
    Match {
        scrutinee: Box<Expr>,
        cases: Vec<MatchCase>,
        else_block: Option<Block>,
    },
    Lambda(Rc<FunDecl>),
}
