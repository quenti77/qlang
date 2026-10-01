use crate::{errors::{LexicalError, QlangError}, utils::Position};

impl QlangError {
    pub fn create_lexical_error(position: Position) -> QlangError {
        QlangError::Lexical(LexicalError { position })
    }
}
