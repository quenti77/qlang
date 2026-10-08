//! Diagnostics: errors are data, never printed by the core.

use crate::span::{Location, SourceMap, Span};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// A secondary message, optionally pointing somewhere else in the code.
#[derive(Clone, Debug, Serialize)]
pub struct Note {
    pub message: String,
    #[serde(skip)]
    pub span: Option<Span>,
    pub location: Option<LocationInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocationInfo {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl From<Location> for LocationInfo {
    fn from(l: Location) -> Self {
        LocationInfo {
            file: l.file,
            line: l.line,
            col: l.col,
            end_line: l.end_line,
            end_col: l.end_col,
        }
    }
}

/// Which stage produced a diagnostic. The code prefix follows the stage:
/// `L` lexer, `P` parser, `T` types/names, `R` runtime.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    #[serde(skip)]
    pub span: Span,
    pub location: Option<LocationInfo>,
    pub notes: Vec<Note>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            code: code.to_string(),
            message: message.into(),
            span,
            location: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: &str, message: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(code, message, span)
        }
    }

    pub fn with_note(mut self, message: impl Into<String>, span: Option<Span>) -> Diagnostic {
        self.notes.push(Note {
            message: message.into(),
            span,
            location: None,
        });
        self
    }

    /// Fill in human-readable locations from the source map.
    pub fn resolve(&mut self, map: &SourceMap) {
        self.location = Some(map.locate(self.span).into());
        for n in &mut self.notes {
            if let Some(s) = n.span {
                n.location = Some(map.locate(s).into());
            }
        }
    }

    /// Render for a terminal: header, source line, underline, notes.
    pub fn render(&self, map: &SourceMap) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let mut out = format!("{sev}[{}]: {}\n", self.code, self.message);
        let loc = map.locate(self.span);
        out.push_str(&format!("  --> {}:{}:{}\n", loc.file, loc.line, loc.col));
        if let Some(text) = map.line_text(self.span.file, loc.line) {
            let gutter = loc.line.to_string();
            let pad = " ".repeat(gutter.len());
            out.push_str(&format!("{pad} |\n{gutter} | {text}\n{pad} | "));
            let start = (loc.col - 1) as usize;
            let len = if loc.end_line == loc.line {
                (loc.end_col.saturating_sub(loc.col)).max(1) as usize
            } else {
                text.chars().count().saturating_sub(start).max(1)
            };
            out.push_str(&" ".repeat(start));
            out.push_str(&"^".repeat(len));
            out.push('\n');
        }
        for n in &self.notes {
            match n.span.map(|s| map.locate(s)) {
                Some(l) => out.push_str(&format!("  note: {} ({}:{}:{})\n", n.message, l.file, l.line, l.col)),
                None => out.push_str(&format!("  note: {}\n", n.message)),
            }
        }
        out
    }
}
