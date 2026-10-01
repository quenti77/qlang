use crate::{errors::LexicalError, utils::Position};

mod scan;

pub struct Token {
    position: Position,
}

pub struct Lexer {}

pub struct LexerResult {
    pub tokens: Vec<String>,
    pub errors: Vec<LexicalError>,
}
