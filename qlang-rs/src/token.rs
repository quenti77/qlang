use crate::position::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    Number,
    String,
    Identifier,
    UnaryOperator,
    BinaryOperator,
    Equals,
    OpenParenthesis,
    CloseParenthesis,
    OpenBrackets,
    CloseBrackets,
    Let,
    If,
    Then,
    Else,
    ElseIf,
    End,
    While,
    For,
    From,
    Until,
    Step,
    Return,
    Break,
    Continue,
    Null,
    Boolean,
    Read,
    Print,
    Function,
    Comma,
    /// Statement introducing a module include (`inclure "chemin.q"`).
    Include,
    #[allow(dead_code)]
    Dot,
    EOF,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub value: String,
    pub position: Position,
}

pub fn create_token(token_type: TokenType, value: impl Into<String>, position: &Position) -> Token {
    Token { token_type, value: value.into(), position: position.clone() }
}

pub fn create_token_at(
    token_type: TokenType,
    value: impl Into<String>,
    index: usize,
    line: usize,
    col: usize,
) -> Token {
    let value = value.into();
    let position = Position::new(index, line, col, value.clone());
    Token { token_type, value, position }
}

pub const KEYWORDS: &[(&str, TokenType)] = &[
    ("dec", TokenType::Let),
    ("si", TokenType::If),
    ("pour", TokenType::For),
    ("de", TokenType::From),
    ("jusque", TokenType::Until),
    ("evol", TokenType::Step),
    ("tantque", TokenType::While),
    ("alors", TokenType::Then),
    ("sinon", TokenType::Else),
    ("sinonsi", TokenType::ElseIf),
    ("fin", TokenType::End),
    ("arreter", TokenType::Break),
    ("continuer", TokenType::Continue),
    ("retour", TokenType::Return),
    ("rien", TokenType::Null),
    ("vrai", TokenType::Boolean),
    ("faux", TokenType::Boolean),
    ("lire", TokenType::Read),
    ("ecrire", TokenType::Print),
    ("et", TokenType::BinaryOperator),
    ("ou", TokenType::BinaryOperator),
    ("non", TokenType::UnaryOperator),
    ("fonction", TokenType::Function),
    ("inclure", TokenType::Include),
];

pub fn lookup_keyword(word: &str) -> Option<TokenType> {
    KEYWORDS.iter().find(|(k, _)| *k == word).map(|(_, t)| *t)
}

pub fn find_keywords_from_token(token_type: TokenType) -> Vec<&'static str> {
    KEYWORDS
        .iter()
        .filter(|(_, t)| *t == token_type)
        .map(|(k, _)| *k)
        .collect()
}

pub const OPERATORS: &[&str] =
    &["+", "-", "*", "/", "%", "==", "!=", ">", "<", ">=", "<=", "et", "ou"];
