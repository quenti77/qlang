//! Lexer: source text to tokens. Comments and blanks are dropped; line
//! breaks are kept as `Newline` tokens because they end statements.

use crate::diag::Diagnostic;
use crate::span::Span;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    /// Raw content between the quotes (escapes and `{}` not yet processed).
    Str(String),
    Ident(String),

    Let,
    Const,
    Fun,
    Return,
    If,
    Then,
    ElseIf,
    Else,
    End,
    While,
    For,
    In,
    Step,
    Do,
    Break,
    Continue,
    Match,
    Case,
    Struct,
    Impl,
    Trait,
    Enum,
    Extends,
    Override,
    Public,
    Private,
    Protected,
    Static,
    Import,
    Export,
    Extern,
    From,
    As,
    And,
    Or,
    Not,
    DivKw,
    ModKw,
    True,
    False,
    None,
    SelfKw,
    Super,

    Power,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    LtEq,
    GtEq,
    Lt,
    Gt,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    /// `div=`
    IntDivAssign,
    /// `mod=`
    ModAssign,
    Assign,
    DotDotEq,
    DotDot,
    Dot,
    Arrow,
    Question,
    Comma,
    Colon,
    LParen,
    RParen,
    LBracket,
    /// `#`, which starts an annotation (`#[name(args)]`).
    Hash,
    RBracket,
    LBrace,
    RBrace,

    Newline,
    Eof,
}

impl Tok {
    /// Human-readable description for error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Int(_) | Tok::Float(_) => "a number".into(),
            Tok::Str(_) => "a string".into(),
            Tok::Ident(n) => format!("`{n}`"),
            Tok::Newline => "end of line".into(),
            Tok::Eof => "end of file".into(),
            other => format!("`{}`", other.text()),
        }
    }

    pub fn text(&self) -> &'static str {
        match self {
            Tok::Let => "let",
            Tok::Const => "const",
            Tok::Fun => "fun",
            Tok::Return => "return",
            Tok::If => "if",
            Tok::Then => "then",
            Tok::ElseIf => "elseif",
            Tok::Else => "else",
            Tok::End => "end",
            Tok::While => "while",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Step => "step",
            Tok::Do => "do",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::Match => "match",
            Tok::Case => "case",
            Tok::Struct => "struct",
            Tok::Impl => "impl",
            Tok::Trait => "trait",
            Tok::Enum => "enum",
            Tok::Extends => "extends",
            Tok::Override => "override",
            Tok::Public => "public",
            Tok::Private => "private",
            Tok::Protected => "protected",
            Tok::Static => "static",
            Tok::Import => "import",
            Tok::Export => "export",
            Tok::Extern => "extern",
            Tok::From => "from",
            Tok::As => "as",
            Tok::And => "and",
            Tok::Or => "or",
            Tok::Not => "not",
            Tok::DivKw => "div",
            Tok::ModKw => "mod",
            Tok::True => "true",
            Tok::False => "false",
            Tok::None => "none",
            Tok::SelfKw => "self",
            Tok::Super => "super",
            Tok::Power => "**",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::EqEq => "==",
            Tok::NotEq => "!=",
            Tok::LtEq => "<=",
            Tok::GtEq => ">=",
            Tok::Lt => "<",
            Tok::Gt => ">",
            Tok::PlusAssign => "+=",
            Tok::MinusAssign => "-=",
            Tok::StarAssign => "*=",
            Tok::SlashAssign => "/=",
            Tok::PercentAssign => "%=",
            Tok::IntDivAssign => "div=",
            Tok::ModAssign => "mod=",
            Tok::Assign => "=",
            Tok::DotDotEq => "..=",
            Tok::DotDot => "..",
            Tok::Dot => ".",
            Tok::Arrow => "->",
            Tok::Question => "?",
            Tok::Comma => ",",
            Tok::Colon => ":",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::Hash => "#",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::Newline => "end of line",
            Tok::Eof => "end of file",
            Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Ident(_) => "?",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "let" => Tok::Let,
        "const" => Tok::Const,
        "fun" => Tok::Fun,
        "return" => Tok::Return,
        "if" => Tok::If,
        "then" => Tok::Then,
        "elseif" => Tok::ElseIf,
        "else" => Tok::Else,
        "end" => Tok::End,
        "while" => Tok::While,
        "for" => Tok::For,
        "in" => Tok::In,
        "step" => Tok::Step,
        "do" => Tok::Do,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "match" => Tok::Match,
        "case" => Tok::Case,
        "struct" => Tok::Struct,
        "impl" => Tok::Impl,
        "trait" => Tok::Trait,
        "enum" => Tok::Enum,
        "extends" => Tok::Extends,
        "override" => Tok::Override,
        "public" => Tok::Public,
        "private" => Tok::Private,
        "protected" => Tok::Protected,
        "static" => Tok::Static,
        "import" => Tok::Import,
        "export" => Tok::Export,
        "extern" => Tok::Extern,
        "from" => Tok::From,
        "as" => Tok::As,
        "and" => Tok::And,
        "or" => Tok::Or,
        "not" => Tok::Not,
        "div" => Tok::DivKw,
        "mod" => Tok::ModKw,
        "true" => Tok::True,
        "false" => Tok::False,
        "none" => Tok::None,
        "self" => Tok::SelfKw,
        "super" => Tok::Super,
        _ => return None,
    })
}

/// A documentation comment (`--" ... --"`) with its text cleaned up, and where it ends.
#[derive(Clone, Debug, PartialEq)]
pub struct DocComment {
    /// Byte offset just after the closing `--"`.
    pub end: usize,
    pub text: String,
}

/// The text of a documentation comment: no blank first and last lines, the common indentation
/// removed. `--" one line --"` gives `one line`.
pub fn clean_doc(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().collect();
    let first = lines.first().map(|l| l.trim_start()).unwrap_or("");
    let rest = &lines[lines.len().min(1)..];
    let indent = rest
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = vec![first.trim_end().to_string()];
    out.extend(rest.iter().map(|l| {
        if l.len() >= indent {
            l[indent..].trim_end().to_string()
        } else {
            l.trim().to_string()
        }
    }));
    lines.clear();
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Lex `src`. `base` is the offset of `src` inside its file (non-zero when
/// lexing the inside of a string interpolation).
pub fn lex(src: &str, file: u32, base: usize, diags: &mut Vec<Diagnostic>) -> Vec<Token> {
    lex_with_docs(src, file, base, diags, &mut Vec::new())
}

/// Like [`lex`], and the documentation comments are collected in `docs`.
pub fn lex_with_docs(
    src: &str,
    file: u32,
    base: usize,
    diags: &mut Vec<Diagnostic>,
    docs: &mut Vec<DocComment>,
) -> Vec<Token> {
    let b = src.as_bytes();
    let mut toks: Vec<Token> = Vec::new();
    let mut i = 0usize;
    let span = |s: usize, e: usize| Span::new(file, base + s, base + e);

    macro_rules! push {
        ($t:expr, $s:expr, $e:expr) => {
            toks.push(Token {
                tok: $t,
                span: span($s, $e),
            })
        };
    }

    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                let s = i;
                i += 1;
                if !matches!(
                    toks.last(),
                    Some(Token {
                        tok: Tok::Newline,
                        ..
                    })
                ) {
                    push!(Tok::Newline, s, i);
                }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => match b.get(i + 2) {
                Some(b'(') => match src[i + 3..].find("--)") {
                    Some(p) => i = i + 3 + p + 3,
                    None => {
                        crate::diag::push_capped(
                            diags,
                            Diagnostic::error(
                                "L003",
                                "unterminated block comment (missing `--)`)",
                                span(i, i + 3),
                            ),
                        );
                        i = b.len();
                    }
                },
                Some(b'"') => match src[i + 3..].find("--\"") {
                    Some(p) => {
                        docs.push(DocComment {
                            end: base + i + 3 + p + 3,
                            text: clean_doc(&src[i + 3..i + 3 + p]),
                        });
                        i = i + 3 + p + 3;
                    }
                    None => {
                        crate::diag::push_capped(
                            diags,
                            Diagnostic::error(
                                "L003",
                                "unterminated documentation comment (missing `--\"`)",
                                span(i, i + 3),
                            ),
                        );
                        i = b.len();
                    }
                },
                _ => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
            },
            // `t.0`, `t.1.0`: after a dot, digits are the position of a tuple element, not a
            // number (so `1.0` is not read as a float there)
            b'0'..=b'9'
                if i > 0
                    && b[i - 1] == b'.'
                    && matches!(toks.last(), Some(Token { tok: Tok::Dot, .. })) =>
            {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                match src[start..i].parse::<i64>() {
                    Ok(n) => push!(Tok::Int(n), start, i),
                    Err(_) => diags.push(Diagnostic::error(
                        "L003",
                        "this tuple position is too large",
                        span(start, i),
                    )),
                }
            }
            b'0'..=b'9' => i = lex_number(src, i, file, base, &mut toks, diags),
            b'"' => {
                let start = i;
                i += 1;
                let content_start = i;
                let closed = match scan_string_end(b, i) {
                    Some(end) => {
                        i = end;
                        true
                    }
                    None => {
                        i = b.len();
                        false
                    }
                };
                let content_end = i.min(b.len());
                if closed {
                    push!(
                        Tok::Str(src[content_start..content_end].to_string()),
                        start,
                        i + 1
                    );
                    i += 1;
                } else {
                    crate::diag::push_capped(
                        diags,
                        Diagnostic::error(
                            "L002",
                            "unterminated string (missing closing `\"`)",
                            span(start, start + 1),
                        ),
                    );
                    push!(
                        Tok::Str(src[content_start..content_end].to_string()),
                        start,
                        content_end
                    );
                    i = b.len();
                }
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &src[start..i];
                // `div=` and `mod=` are compound assignments (but `div ==` is not)
                let compound = matches!(word, "div" | "mod")
                    && b.get(i) == Some(&b'=')
                    && b.get(i + 1) != Some(&b'=');
                if compound {
                    i += 1;
                }
                let tok = match (word, compound) {
                    ("div", true) => Tok::IntDivAssign,
                    ("mod", true) => Tok::ModAssign,
                    _ => keyword(word).unwrap_or_else(|| Tok::Ident(word.to_string())),
                };
                push!(tok, start, i);
            }
            _ => {
                let rest = &b[i..];
                let (tok, len) = if rest.starts_with(b"..=") {
                    (Some(Tok::DotDotEq), 3)
                } else if rest.starts_with(b"**") {
                    (Some(Tok::Power), 2)
                } else if rest.starts_with(b"==") {
                    (Some(Tok::EqEq), 2)
                } else if rest.starts_with(b"!=") {
                    (Some(Tok::NotEq), 2)
                } else if rest.starts_with(b"<=") {
                    (Some(Tok::LtEq), 2)
                } else if rest.starts_with(b">=") {
                    (Some(Tok::GtEq), 2)
                } else if rest.starts_with(b"+=") {
                    (Some(Tok::PlusAssign), 2)
                } else if rest.starts_with(b"-=") {
                    (Some(Tok::MinusAssign), 2)
                } else if rest.starts_with(b"*=") {
                    (Some(Tok::StarAssign), 2)
                } else if rest.starts_with(b"/=") {
                    (Some(Tok::SlashAssign), 2)
                } else if rest.starts_with(b"%=") {
                    (Some(Tok::PercentAssign), 2)
                } else if rest.starts_with(b"..") {
                    (Some(Tok::DotDot), 2)
                } else if rest.starts_with(b"->") {
                    (Some(Tok::Arrow), 2)
                } else {
                    (
                        match c {
                            b'+' => Some(Tok::Plus),
                            b'-' => Some(Tok::Minus),
                            b'*' => Some(Tok::Star),
                            b'/' => Some(Tok::Slash),
                            b'%' => Some(Tok::Percent),
                            b'<' => Some(Tok::Lt),
                            b'>' => Some(Tok::Gt),
                            b'=' => Some(Tok::Assign),
                            b'.' => Some(Tok::Dot),
                            b'?' => Some(Tok::Question),
                            b',' => Some(Tok::Comma),
                            b':' => Some(Tok::Colon),
                            b'(' => Some(Tok::LParen),
                            b')' => Some(Tok::RParen),
                            b'#' => Some(Tok::Hash),
                            b'[' => Some(Tok::LBracket),
                            b']' => Some(Tok::RBracket),
                            b'{' => Some(Tok::LBrace),
                            b'}' => Some(Tok::RBrace),
                            _ => None,
                        },
                        1,
                    )
                };
                match tok {
                    Some(t) => {
                        push!(t, i, i + len);
                        i += len;
                    }
                    None => {
                        // one full character, possibly multi-byte
                        let ch = src[i..].chars().next().unwrap();
                        let msg = match ch {
                            '!' => "unexpected `!` (use `not` for negation, `!=` for inequality)"
                                .to_string(),
                            '&' => "unexpected `&` (use `and`)".to_string(),
                            '|' => "unexpected `|` (use `or`)".to_string(),
                            ';' => {
                                "unexpected `;` (statements end at the end of the line)".to_string()
                            }
                            '\'' => "unexpected `'` (strings use double quotes)".to_string(),
                            c => format!("unexpected character `{c}`"),
                        };
                        crate::diag::push_capped(
                            diags,
                            Diagnostic::error("L001", msg, span(i, i + ch.len_utf8())),
                        );
                        i += ch.len_utf8();
                    }
                }
            }
        }
    }
    toks.push(Token {
        tok: Tok::Eof,
        span: span(src.len(), src.len()),
    });
    toks
}

fn lex_number(
    src: &str,
    start: usize,
    file: u32,
    base: usize,
    toks: &mut Vec<Token>,
    diags: &mut Vec<Diagnostic>,
) -> usize {
    let b = src.as_bytes();
    let span = |s: usize, e: usize| Span::new(file, base + s, base + e);
    let mut i = start;

    // digits with single underscores between digits
    let eat_digits = |i: &mut usize, ok: fn(u8) -> bool| {
        let mut text = String::new();
        while *i < b.len() {
            if ok(b[*i]) {
                text.push(b[*i] as char);
                *i += 1;
            } else if b[*i] == b'_' && !text.is_empty() && b.get(*i + 1).is_some_and(|c| ok(*c)) {
                *i += 1;
            } else {
                break;
            }
        }
        text
    };

    if b[i] == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X' | b'b' | b'B')) {
        let hex = matches!(b[i + 1], b'x' | b'X');
        i += 2;
        let digits = if hex {
            eat_digits(&mut i, |c| c.is_ascii_hexdigit())
        } else {
            eat_digits(&mut i, |c| c == b'0' || c == b'1')
        };
        if digits.is_empty() {
            crate::diag::push_capped(
                diags,
                Diagnostic::error(
                    "L004",
                    if hex {
                        "expected hexadecimal digits after `0x`"
                    } else {
                        "expected binary digits (0 or 1) after `0b`"
                    },
                    span(start, i),
                ),
            );
            toks.push(Token {
                tok: Tok::Int(0),
                span: span(start, i),
            });
            return i;
        }
        match i64::from_str_radix(&digits, if hex { 16 } else { 2 }) {
            Ok(v) => toks.push(Token {
                tok: Tok::Int(v),
                span: span(start, i),
            }),
            Err(_) => {
                crate::diag::push_capped(
                    diags,
                    Diagnostic::error("L005", "integer literal is too large", span(start, i)),
                );
                toks.push(Token {
                    tok: Tok::Int(0),
                    span: span(start, i),
                });
            }
        }
        return i;
    }

    let mut text = eat_digits(&mut i, |c| c.is_ascii_digit());
    let mut is_float = false;
    // a float needs a digit after the dot: `0..10` is a range
    if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
        is_float = true;
        i += 1;
        text.push('.');
        text.push_str(&eat_digits(&mut i, |c| c.is_ascii_digit()));
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            text.push('e');
            if matches!(b[i + 1], b'+' | b'-') {
                text.push(b[i + 1] as char);
            }
            i = j;
            text.push_str(&eat_digits(&mut i, |c| c.is_ascii_digit()));
        }
    }
    if is_float {
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => toks.push(Token {
                tok: Tok::Float(v),
                span: span(start, i),
            }),
            _ => {
                crate::diag::push_capped(
                    diags,
                    Diagnostic::error("L005", "float literal is out of range", span(start, i)),
                );
                toks.push(Token {
                    tok: Tok::Float(0.0),
                    span: span(start, i),
                });
            }
        }
    } else {
        match text.parse::<i64>() {
            Ok(v) => toks.push(Token {
                tok: Tok::Int(v),
                span: span(start, i),
            }),
            Err(_) => {
                crate::diag::push_capped(
                    diags,
                    Diagnostic::error("L005", "integer literal is too large", span(start, i)),
                );
                toks.push(Token {
                    tok: Tok::Int(0),
                    span: span(start, i),
                });
            }
        }
    }
    i
}

/// The index of the quote that closes a string whose content starts at `i`. A `{...}` inside
/// the string is code, and may hold strings of its own: `"{names.join(", ")}"`.
pub(crate) fn scan_string_end(b: &[u8], mut i: usize) -> Option<usize> {
    let mut depth = 0usize;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'"' if depth == 0 => return Some(i),
            b'"' => i = scan_string_end(b, i + 1)? + 1,
            _ => i += 1,
        }
    }
    None
}
