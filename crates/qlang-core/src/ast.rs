//! Syntax tree produced by the parser.

use crate::span::Span;
use std::rc::Rc;

pub type NodeId = u32;

#[derive(Debug)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Import(Import),
    Export(Export),
    Fun(Rc<FunDecl>),
    Struct(StructDecl),
    Impl(ImplDecl),
    Trait(TraitDecl),
    Enum(EnumDecl),
    Stmt(Stmt),
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

#[derive(Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeExpr,
}

#[derive(Debug)]
pub struct FunDecl {
    pub id: NodeId,
    pub span: Span,
    pub name: Option<Ident>,
    pub tparams: Vec<TypeParam>,
    /// `self` appeared as the first parameter.
    pub has_self: bool,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    /// `None` for trait signatures.
    pub body: Option<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug)]
pub struct FieldDecl {
    pub vis: Option<Visibility>,
    pub is_static: bool,
    pub name: Ident,
    pub ty: TypeExpr,
    pub default: Option<Rc<Expr>>,
}

#[derive(Debug)]
pub struct StructDecl {
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
    pub span: Span,
    pub name: Ident,
    pub tparams: Vec<TypeParam>,
    pub sigs: Vec<FunDecl>,
}

#[derive(Debug)]
pub struct EnumDecl {
    pub span: Span,
    pub name: Ident,
    pub variants: Vec<Ident>,
}

#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub span: Span,
    pub kind: TypeExprKind,
}

#[derive(Clone, Debug)]
pub enum TypeExprKind {
    /// `a.b.C<args>`
    Named { path: Vec<Ident>, args: Vec<TypeExpr> },
    Nullable(Box<TypeExpr>),
    Fun { params: Vec<TypeExpr>, ret: Option<Box<TypeExpr>> },
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

#[derive(Debug)]
pub enum StmtKind {
    Let {
        is_const: bool,
        name: Ident,
        ty: Option<TypeExpr>,
        init: Expr,
    },
    Return(Option<Expr>),
    While { cond: Expr, body: Block },
    For {
        var: Ident,
        /// `for key, value in map` / `for index, item in array`
        var2: Option<Ident>,
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
    Range { lo: Box<Pattern>, hi: Box<Pattern>, inclusive: bool },
    /// `Color.Red`, `m.Color.Red`
    Path(Vec<Ident>),
    /// `x`: matches anything and binds the value.
    Bind(Ident),
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
    Call { callee: Box<Expr>, args: Vec<Expr> },
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
