/// Explicit receiver parameter name (Python/Go/Rust-style `self`): a method
/// declaring it as its first parameter is an instance method, one that
/// omits it is a static method - no separate `statique` keyword needed.
pub const SELF_PARAM: &str = "moi";

/// Field/method visibility for the `structure`/`implemente` object system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// `publique` - accessible from anywhere.
    Public,
    /// `cacher` - accessible only from a method of the same structure.
    Hidden,
    /// `partager` - accessible only from a method of the same structure
    /// (kept distinct from `Hidden` for future subtype support).
    Shared,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub visibility: Visibility,
    pub name: String,
    /// `publique champ = valeur` - evaluated fresh for every `Nom()` call,
    /// in the scope where the struct was declared. `rien` when absent.
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodDecl {
    pub visibility: Visibility,
    pub is_static: bool,
    pub function: FunctionDecl,
}

/// An anonymous or named function declaration, usable both as a statement
/// (`fonction nom(...) ... fin`) and as an expression (passed as an
/// argument, or assigned to a variable).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub identifier: Option<String>,
    pub parameters: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VariableDeclaration {
        identifier: String,
        value: Option<Expr>,
    },
    Print(Expr),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    For {
        identifier: String,
        from: Expr,
        until: Expr,
        step: Expr,
        body: Box<Stmt>,
    },
    Function(FunctionDecl),
    Block(Vec<Stmt>),
    Break,
    Continue,
    Return(Expr),
    /// `inclure <expression evaluating to a path>` - loads another qlang
    /// source file as a module and merges its top-level declarations into
    /// the current environment.
    Include(Expr),
    /// `structure Nom avec ... fin` - declares a struct's fields.
    Struct {
        name: String,
        fields: Vec<StructField>,
    },
    /// `dans Nom implemente ... fin` - attaches methods to a struct.
    Impl {
        name: String,
        methods: Vec<MethodDecl>,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Assignment {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Read(Box<Expr>),
    Unary {
        operator: String,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        right: Box<Expr>,
        operator: String,
    },
    Numeric(f64),
    Str(String),
    Null,
    Boolean(bool),
    Identifier(String),
    Member {
        object: Box<Expr>,
        property: Option<Box<Expr>>,
    },
    Array(Vec<Expr>),
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Function(FunctionDecl),
}

pub type Program = Vec<Stmt>;
