use crate::{lexer::Token, utils::Position};

mod create;

pub struct LexicalError {
    position: Position,
}

pub struct ParseError {
    token: Token,
}

pub struct RuntimeError {}

pub enum QlangError {
    Lexical(LexicalError),
    Parse(ParseError),
    Runtime(RuntimeError),
}
