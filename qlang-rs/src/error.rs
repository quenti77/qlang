use std::fmt;

use crate::position::Position;

/// A source span used to render the `^^^^` error underline.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

/// Shared payload for every source-anchored error variant. Boxed inside
/// `QError` so the `Result<T, QError>` returned all over the lexer/parser
/// stays small regardless of how much position/message data an error carries.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceError {
    pub span: Span,
    pub details: String,
    pub code: String,
}

/// All errors the qlang language itself can produce: lexing, parsing and
/// evaluation failures, plus module-resolution failures for `inclure`.
///
/// This type is deliberately free of anything related to a specific host
/// (CLI, GUI, embedder, ...): it only carries structured data (a kind, a
/// message and, when relevant, a source span) and implements the standard
/// `Display`/`Error` traits. Any embedding tool can therefore match on the
/// variant, call `render()` for a human-readable multi-line diagnostic, or
/// simply propagate it with `?` like any other Rust error.
#[derive(Debug, Clone, PartialEq)]
pub enum QError {
    IllegalChar(Box<SourceError>),
    StringUnterminated(Box<SourceError>),
    InvalidSyntax(Box<SourceError>),
    MaximumArgument(Box<SourceError>),
    Runtime { message: String },
    Module { message: String },
}

impl QError {
    pub fn illegal_char(start: Position, end: Position, details: impl Into<String>, code: impl Into<String>) -> Self {
        Self::IllegalChar(Box::new(SourceError {
            span: Span { start, end },
            details: details.into(),
            code: code.into(),
        }))
    }

    pub fn string_unterminated(start: Position, end: Position, code: impl Into<String>) -> Self {
        Self::StringUnterminated(Box::new(SourceError {
            span: Span { start, end },
            details: String::new(),
            code: code.into(),
        }))
    }

    pub fn invalid_syntax(start: Position, end: Position, details: impl Into<String>, code: impl Into<String>) -> Self {
        Self::InvalidSyntax(Box::new(SourceError {
            span: Span { start, end },
            details: details.into(),
            code: code.into(),
        }))
    }

    pub fn maximum_argument(start: Position, end: Position, details: impl Into<String>, code: impl Into<String>) -> Self {
        Self::MaximumArgument(Box::new(SourceError {
            span: Span { start, end },
            details: details.into(),
            code: code.into(),
        }))
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime { message: message.into() }
    }

    pub fn module(message: impl Into<String>) -> Self {
        Self::Module { message: message.into() }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::IllegalChar(_) => "Charactère non valide",
            Self::StringUnterminated(_) => "Chaîne non terminée",
            Self::InvalidSyntax(_) => "Syntaxe invalide",
            Self::MaximumArgument(_) => "Nombre d'arguments maximum dépassé",
            Self::Runtime { .. } => "Erreur d'exécution",
            Self::Module { .. } => "Erreur de module",
        }
    }

    pub fn details(&self) -> String {
        match self {
            Self::IllegalChar(err) => format!("'{}'", err.details),
            Self::StringUnterminated(_) => "La chaîne n'est pas terminée".to_string(),
            Self::InvalidSyntax(err) => err.details.clone(),
            Self::MaximumArgument(err) => err.details.clone(),
            Self::Runtime { message } => message.clone(),
            Self::Module { message } => message.clone(),
        }
    }

    fn source_error(&self) -> Option<&SourceError> {
        match self {
            Self::IllegalChar(err)
            | Self::StringUnterminated(err)
            | Self::InvalidSyntax(err)
            | Self::MaximumArgument(err) => Some(err),
            Self::Runtime { .. } | Self::Module { .. } => None,
        }
    }

    /// Renders a multi-line, human-readable diagnostic (name, position and
    /// the offending source line with a `^^^` underline), mirroring the
    /// original TS `QError.render()`. Purely a convenience: a host is free
    /// to ignore it and build its own presentation from the structured data.
    pub fn render(&self) -> Vec<String> {
        let mut result = format!("{}: {}\n", self.name(), self.details());

        if let Some(err) = self.source_error() {
            result += &format!("Sur la line {} et colonne {}\n \n", err.span.end.line, err.span.end.col);
            result += &with_arrows(&err.span.start, &err.span.end, &err.code);
        }

        result.split('\n').map(|s| s.to_string()).collect()
    }
}

fn with_arrows(start: &Position, end: &Position, code: &str) -> String {
    let line = code
        .split('\n')
        .nth(start.line.saturating_sub(1))
        .unwrap_or("");
    let spaces = " ".repeat(start.col.saturating_sub(1));
    let arrows = "^".repeat(end.col.saturating_sub(start.col));
    format!("{line}\n{spaces}{arrows}")
}

impl fmt::Display for QError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name(), self.details())
    }
}

impl std::error::Error for QError {}
