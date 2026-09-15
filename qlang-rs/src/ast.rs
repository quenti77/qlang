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
    VariableDeclaration { identifier: String, value: Option<Expr> },
    Print(Expr),
    If { condition: Expr, then_branch: Box<Stmt>, else_branch: Option<Box<Stmt>> },
    While { condition: Expr, body: Box<Stmt> },
    For { identifier: String, from: Expr, until: Expr, step: Expr, body: Box<Stmt> },
    Function(FunctionDecl),
    Block(Vec<Stmt>),
    Break,
    Continue,
    Return(Expr),
    /// `inclure <expression evaluating to a path>` - loads another qlang
    /// source file as a module and merges its top-level declarations into
    /// the current environment.
    Include(Expr),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Assignment { target: Box<Expr>, value: Box<Expr> },
    Read(Box<Expr>),
    Unary { operator: String, value: Box<Expr> },
    Binary { left: Box<Expr>, right: Box<Expr>, operator: String },
    Numeric(f64),
    Str(String),
    Null,
    Boolean(bool),
    Identifier(String),
    Member { object: Box<Expr>, property: Option<Box<Expr>> },
    Array(Vec<Expr>),
    Call { callee: Box<Expr>, arguments: Vec<Expr> },
    Function(FunctionDecl),
}

pub type Program = Vec<Stmt>;
