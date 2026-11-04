//! `qlang fmt`: the layout of a program, four spaces for each level of nesting.
//!
//! Only **white space** changes: at the start of lines (the level), at their end, and between two
//! tokens of a line (see [`gap`]). The tokens and the comments stay as they are, so a formatted
//! program means exactly what it meant. That is checked on every run: the tokens of the result must
//! be the tokens of the source, or nothing is returned.
//!
//! The level of a line is the number of lines that opened a block (`fun`, `if`, `match`, `struct`...)
//! or a bracket that is still open, so `foo(fun(x: int) -> int` followed by its body and `end)`
//! gives one level, not two. Lines inside a multi-line string or comment are left alone.

use crate::diag::Diagnostic;
use crate::lexer::{Tok, Token, lex};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// `fun`, `if`, `while`, `for`, `struct`, `impl`, `trait`, `enum`: closed by `end`.
    Block,
    If,
    Match,
    Trait,
    Impl,
    /// `(`, `[`, `{`.
    Paren,
    /// `{`: a literal.
    Brace,
    /// The `(` of a parameter list.
    Params,
    Struct,
    Enum,
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    kind: Kind,
    line: usize,
    /// A `match` whose arm has its body on the following lines.
    arm: bool,
}

/// The source, laid out. `Err` when it does not lex, or (never expected) when the layout would
/// change the tokens.
pub fn format(src: &str) -> Result<String, String> {
    let once = layout(src)?;
    // A `\r` that is not part of a `\r\n` can only be inside a string or a comment, where the layout keeps
    // it; next to the line breaks it adds or drops, it could make a second pass differ. Nothing is
    // written rather than a text that changes each time.
    if once.replace("\r\n", "").contains('\r') && layout(&once).as_deref() != Ok(once.as_str()) {
        return Err("a lone carriage return makes the layout unstable: nothing was written".into());
    }
    Ok(once)
}

fn layout(src: &str) -> Result<String, String> {
    probe(src).map(|l| l.text)
}

/// What the layout found out about a text, besides the text itself.
struct Probe {
    text: String,
    /// The level of each line that has code on it.
    levels: Vec<Option<usize>>,
    /// The lines that left more blocks open than they found.
    opens: Vec<bool>,
    /// The blocks (`fun`, `if`, `match`...) still open at the end of the text.
    open_blocks: usize,
}

fn is_block(kind: Kind) -> bool {
    !matches!(kind, Kind::Paren | Kind::Brace | Kind::Params)
}

fn probe(src: &str) -> Result<Probe, String> {
    let crlf = src.contains("\r\n");
    let text = src.replace("\r\n", "\n");
    let mut diags: Vec<Diagnostic> = Vec::new();
    let toks = lex(&text, 0, 0, &mut diags);
    if let Some(d) = diags.first() {
        return Err(format!("the program does not lex: {}", d.message));
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut starts: Vec<usize> = Vec::with_capacity(lines.len());
    let mut at = 0usize;
    for l in &lines {
        starts.push(at);
        at += l.len() + 1;
    }
    let line_of = |offset: usize| match starts.binary_search(&offset) {
        Ok(i) => i,
        Err(i) => i - 1,
    };

    // lines that belong to a multi-line token or comment: left exactly as they are
    let mut verbatim = vec![false; lines.len()];
    let real: Vec<&Token> = toks
        .iter()
        .filter(|t| !matches!(t.tok, Tok::Newline | Tok::Eof))
        .collect();
    for t in &real {
        let (a, b) = (
            line_of(t.span.start as usize),
            line_of((t.span.end as usize).saturating_sub(1)),
        );
        for v in verbatim.iter_mut().take(b + 1).skip(a + 1) {
            *v = true;
        }
    }
    // comments live in the gaps between tokens
    let mut gap_start = 0usize;
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    for t in &real {
        gaps.push((gap_start, t.span.start as usize));
        gap_start = t.span.end as usize;
    }
    gaps.push((gap_start, text.len()));
    for (from, to) in gaps {
        let gap = &text[from..to];
        let b = gap.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'-' && b.get(i + 1) == Some(&b'-') {
                let begin = from + i;
                let end = match b.get(i + 2) {
                    Some(b'(') => gap[i + 3..].find("--)").map(|p| i + 3 + p + 3),
                    Some(b'"') => gap[i + 3..].find("--\"").map(|p| i + 3 + p + 3),
                    _ => gap[i..].find('\n').map(|p| i + p),
                }
                .unwrap_or(b.len());
                let (la, lb) = (
                    line_of(begin),
                    line_of((from + end).saturating_sub(1).max(begin)),
                );
                for v in verbatim.iter_mut().take(lb + 1).skip(la + 1) {
                    *v = true;
                }
                i = end;
            } else {
                i += 1;
            }
        }
    }

    // the tokens of each line
    let mut per_line: Vec<Vec<usize>> = vec![Vec::new(); lines.len()];
    for (k, t) in real.iter().enumerate() {
        per_line[line_of(t.span.start as usize)].push(k);
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut level_of: Vec<Option<usize>> = vec![None; lines.len()];
    let mut opens: Vec<bool> = vec![false; lines.len()];
    let mut closer_first: Vec<bool> = vec![false; lines.len()];
    let mut prev_last: Option<Tok> = None;
    let mut impl_header = false;
    let mut case_header = false;
    let mut pending_params = false;
    let mut type_until = 0usize;
    for (n, idxs) in per_line.iter().enumerate() {
        if idxs.is_empty() {
            continue;
        }
        let first = &real[idxs[0]].tok;
        let type_alias = matches!(first, Tok::Ident(name) if name == "type");
        let blocks_before = stack.iter().filter(|f| is_block(f.kind)).count();
        // closers at the start of the line leave their blocks before the level is computed
        let mut done = 0usize;
        let mut lead = stack.clone();
        while done < idxs.len() {
            match &real[idxs[done]].tok {
                Tok::End | Tok::RParen | Tok::RBracket | Tok::RBrace => {
                    lead.pop();
                    done += 1;
                }
                _ => break,
            }
        }
        closer_first[n] = done > 0;
        let mut frames = lead;
        // `else` of an `if` is level with the `if`; a `case` or the `else` of a `match` is one level
        // in, and its body one more
        let mut except_top = false;
        let mut ignore_arm = false;
        if done == 0 {
            let top = frames.last().map(|f| f.kind);
            match first {
                Tok::Case => ignore_arm = top == Some(Kind::Match),
                Tok::Else | Tok::ElseIf => match top {
                    Some(Kind::If) => except_top = true,
                    Some(Kind::Match) => ignore_arm = true,
                    _ => {}
                },
                _ => {}
            }
        }
        let mut level = distinct_lines(&frames, except_top);
        level += frames
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                f.kind == Kind::Match && f.arm && !(ignore_arm && *i + 1 == frames.len())
            })
            .count();
        // a line that goes on from an operator, or starts with one
        let continues = matches!(prev_last, Some(ref t) if is_continuation(t));
        let leads_on = matches!(first, Tok::Dot | Tok::And | Tok::Or);
        if (continues || leads_on) && done == 0 && !(continues && leads_on) {
            level += 1;
        }
        level_of[n] = Some(level);
        frames.clear();

        // now run the whole line through the stack
        let mut last_tok: Option<&Tok> = None;
        for (j, &k) in idxs.iter().enumerate() {
            let t = &real[k].tok;
            let prev = if k > 0 { Some(&real[k - 1].tok) } else { None };
            let next = real.get(k + 1).map(|t| &t.tok);
            let first_of_stmt = j == 0;
            match t {
                Tok::Fun => {
                    let named = matches!(next, Some(Tok::Ident(_)));
                    let in_trait = stack.last().is_some_and(|f| f.kind == Kind::Trait);
                    let after_extern = matches!(prev, Some(Tok::Extern));
                    let in_type = k < type_until;
                    let opens = if named {
                        !in_trait && !after_extern
                    } else {
                        !in_type
                    };
                    if opens {
                        stack.push(Frame {
                            kind: Kind::Block,
                            line: n,
                            arm: false,
                        });
                    }
                    // the `(` that follows holds the parameters, of a signature too
                    if !in_type {
                        pending_params = true;
                    }
                }
                Tok::If => {
                    if case_header {
                        // `case x if x > 0 then`: a guard, not a block
                    } else {
                        stack.push(Frame {
                            kind: Kind::If,
                            line: n,
                            arm: false,
                        });
                    }
                }
                Tok::While => stack.push(Frame {
                    kind: Kind::Block,
                    line: n,
                    arm: false,
                }),
                Tok::For => {
                    if impl_header {
                        // `impl Trait for Type`
                    } else {
                        stack.push(Frame {
                            kind: Kind::Block,
                            line: n,
                            arm: false,
                        });
                    }
                }
                Tok::Struct => stack.push(Frame {
                    kind: Kind::Struct,
                    line: n,
                    arm: false,
                }),
                Tok::Enum => stack.push(Frame {
                    kind: Kind::Enum,
                    line: n,
                    arm: false,
                }),
                Tok::Trait => stack.push(Frame {
                    kind: Kind::Trait,
                    line: n,
                    arm: false,
                }),
                Tok::Impl => {
                    stack.push(Frame {
                        kind: Kind::Impl,
                        line: n,
                        arm: false,
                    });
                    impl_header = true;
                }
                Tok::Match => stack.push(Frame {
                    kind: Kind::Match,
                    line: n,
                    arm: false,
                }),
                Tok::Colon => {
                    let top = stack.last().map(|f| f.kind);
                    let parent = if stack.len() >= 2 {
                        Some(stack[stack.len() - 2].kind)
                    } else {
                        None
                    };
                    let declares = matches!(top, Some(Kind::Params) | Some(Kind::Struct))
                        || (top == Some(Kind::Paren) && parent == Some(Kind::Enum))
                        || (!matches!(top, Some(Kind::Paren) | Some(Kind::Brace))
                            && matches!(real[idxs[0]].tok, Tok::Let | Tok::Const));
                    if declares {
                        type_until = skip_type(&real, k + 1);
                    }
                }
                Tok::Arrow | Tok::As => type_until = skip_type(&real, k + 1),
                // `Reflect.find<fun(Request) -> Response>(name)`: a type in the arguments of a call
                Tok::Lt if matches!(next, Some(Tok::Fun)) => type_until = skip_type(&real, k + 1),
                Tok::Assign => {
                    if type_alias {
                        type_until = skip_type(&real, k + 1);
                    }
                }
                Tok::Case => {
                    if first_of_stmt && stack.last().is_some_and(|f| f.kind == Kind::Match) {
                        case_header = true;
                    }
                }
                Tok::Then => {
                    if case_header {
                        case_header = false;
                        let rest = idxs.len() == j + 1;
                        if let Some(f) = stack.last_mut()
                            && f.kind == Kind::Match
                        {
                            f.arm = rest;
                        }
                    }
                }
                Tok::Else => {
                    if let Some(f) = stack.last_mut()
                        && f.kind == Kind::Match
                    {
                        f.arm = idxs.len() == j + 1;
                    }
                }
                Tok::End => {
                    stack.pop();
                }
                Tok::LParen => {
                    let kind = if pending_params {
                        Kind::Params
                    } else {
                        Kind::Paren
                    };
                    pending_params = false;
                    stack.push(Frame {
                        kind,
                        line: n,
                        arm: false,
                    });
                }
                Tok::LBracket => stack.push(Frame {
                    kind: Kind::Paren,
                    line: n,
                    arm: false,
                }),
                Tok::LBrace => stack.push(Frame {
                    kind: Kind::Brace,
                    line: n,
                    arm: false,
                }),
                Tok::RParen | Tok::RBracket | Tok::RBrace => {
                    stack.pop();
                }
                _ => {}
            }
            last_tok = Some(t);
        }
        opens[n] = stack.iter().filter(|f| is_block(f.kind)).count() > blocks_before;
        impl_header = false;
        prev_last = last_tok.cloned();
        case_header = false;
    }

    // comments take the level of the code they sit in front of
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut above = 0usize;
    let mut blanks = 0usize;
    for (n, line) in lines.iter().enumerate() {
        let spaced = if verbatim[n] {
            None
        } else {
            let on_line: Vec<&Token> = per_line[n].iter().map(|&k| real[k]).collect();
            tighten(&text, starts[n], line, &on_line)
        };
        let body = spaced
            .as_deref()
            .unwrap_or_else(|| line.trim_start_matches([' ', '\t']));
        if verbatim[n] {
            out.push((*line).to_string());
            blanks = 0;
            continue;
        }
        if body.trim_end().is_empty() {
            blanks += 1;
            if blanks == 1 && !out.is_empty() {
                out.push(String::new());
            }
            continue;
        }
        blanks = 0;
        let level = match level_of[n] {
            Some(l) => {
                above = l;
                l
            }
            None => {
                // a comment: the level of the next code line; in front of an `end` or a closing
                // bracket it is still inside the block
                let mut k = n + 1;
                while k < lines.len() && level_of[k].is_none() {
                    k += 1;
                }
                match level_of.get(k).copied().flatten() {
                    Some(l) if closer_first[k] => l + 1,
                    Some(l) => l,
                    None => above,
                }
            }
        };
        let mut s = "    ".repeat(level);
        if level_of[n].is_none() && is_multiline_comment_start(&lines, n, &verbatim) {
            s.push_str(body);
        } else {
            s.push_str(body.trim_end());
        }
        out.push(s);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let open_blocks = stack.iter().filter(|f| is_block(f.kind)).count();
    if out.is_empty() {
        return Ok(Probe {
            text: String::new(),
            levels: level_of,
            opens,
            open_blocks,
        });
    }
    let mut result = out.join("\n");
    result.push('\n');

    // nothing but white space may have changed
    let mut d2: Vec<Diagnostic> = Vec::new();
    let again = lex(&result, 0, 0, &mut d2);
    // line breaks at the very start or end carry no meaning
    let core = |t: &[Token]| -> Vec<Tok> {
        let mut v: Vec<Tok> = t.iter().map(|t| t.tok.clone()).collect();
        while matches!(v.first(), Some(Tok::Newline)) {
            v.remove(0);
        }
        while matches!(v.last(), Some(Tok::Eof | Tok::Newline)) {
            v.pop();
        }
        v
    };
    let same = |a: &[Token], b: &[Token]| core(a) == core(b);
    if !d2.is_empty() || !same(&toks, &again) {
        return Err(
            "the layout would change the program: nothing was written (please report this)".into(),
        );
    }
    // A lone `\r` at the end of a line the layout kept as it was (inside a string or a comment) meets
    // the line break written after it: the text is then a CRLF text, and is written as one.
    if crlf || result.contains("\r\n") {
        result = result.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    Ok(Probe {
        text: result,
        levels: level_of,
        opens,
        open_blocks,
    })
}

/// What an editor needs to know while the program is being typed.
pub struct Typing {
    /// The level (four spaces each) the line should be at.
    pub level: usize,
    /// The line opened a block (`fun`, `if`, `match`...) that nothing closes yet.
    pub unclosed: bool,
}

/// The level of line `line` (counted from 0) and whether it opens a block left open, for the layout
/// as you type: an empty line is taken as if it had a statement on it, and the text may have blocks
/// without their `end`. `None` when the text does not lex or the line is a comment.
pub fn typing(src: &str, line: usize) -> Option<Typing> {
    let text = src.replace("\r\n", "\n");
    let mut lines: Vec<&str> = text.split('\n').collect();
    if line >= lines.len() {
        return None;
    }
    if lines[line].trim().is_empty() {
        lines[line] = "_";
    }
    let p = probe(&lines.join("\n")).ok()?;
    Some(Typing {
        level: p.levels.get(line).copied().flatten()?,
        unclosed: p.opens[line] && p.open_blocks > 0,
    })
}

/// The line with the white space between its tokens put in order, or `None` when it has nothing
/// to change: no token, a comment in front of the first one, or only gaps that are already right.
/// What follows the last token (a comment, the rest of a string running over several lines) stays.
fn tighten(text: &str, line_start: usize, line: &str, toks: &[&Token]) -> Option<String> {
    let first = toks.first()?;
    let line_end = line_start + line.len();
    let begin = first.span.start as usize;
    if !text[line_start..begin]
        .trim_start_matches([' ', '\t'])
        .is_empty()
    {
        return None;
    }
    let mut out = String::with_capacity(line.len());
    for pair in toks.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (end, next) = (a.span.end as usize, b.span.start as usize);
        out.push_str(&text[a.span.start as usize..end]);
        let old = &text[end..next];
        if old.chars().all(|c| c == ' ' || c == '\t') {
            out.push_str(gap(&a.tok, &b.tok, old.is_empty()));
        } else {
            out.push_str(old);
        }
    }
    let last = toks[toks.len() - 1].span.start as usize;
    out.push_str(&text[last..line_end]);
    let before = &text[begin..line_end];
    (out != before).then_some(out)
}

/// The white space between two tokens of a line. One space, except that nothing goes inside a
/// bracket, before a `,` or a `.`, or between a name and the `(` or `[` of its call or index (and
/// between `fun` and the `(` of a function without a name); a `,` is followed by a space. Where
/// there was none, nothing is added, apart from after a `,`.
fn gap(a: &Tok, b: &Tok, none_before: bool) -> &'static str {
    let tight = matches!(a, Tok::LParen | Tok::LBracket | Tok::Dot | Tok::Ellipsis)
        || matches!(b, Tok::RParen | Tok::RBracket | Tok::Comma | Tok::Dot)
        || (matches!(b, Tok::LParen | Tok::LBracket)
            && matches!(
                a,
                Tok::Ident(_) | Tok::RParen | Tok::RBracket | Tok::Fun | Tok::SelfKw | Tok::Super
            ));
    if tight || (none_before && !matches!(a, Tok::Comma)) {
        ""
    } else {
        " "
    }
}

/// A comment line that starts a comment running over several lines keeps its own text.
fn is_multiline_comment_start(lines: &[&str], n: usize, verbatim: &[bool]) -> bool {
    n + 1 < lines.len() && verbatim[n + 1]
}

fn distinct_lines(frames: &[Frame], except_top: bool) -> usize {
    let end = if except_top && !frames.is_empty() {
        frames.len() - 1
    } else {
        frames.len()
    };
    let mut count = 0;
    let mut last: Option<usize> = None;
    for f in &frames[..end] {
        if last != Some(f.line) {
            count += 1;
            last = Some(f.line);
        }
    }
    count
}

fn is_continuation(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::Percent
            | Tok::Power
            | Tok::EqEq
            | Tok::NotEq
            | Tok::LtEq
            | Tok::GtEq
            | Tok::And
            | Tok::Or
            | Tok::Assign
            | Tok::PlusAssign
            | Tok::MinusAssign
            | Tok::StarAssign
            | Tok::SlashAssign
            | Tok::Arrow
            | Tok::Dot
            | Tok::DivKw
            | Tok::ModKw
            | Tok::QuestionQuestion
            | Tok::Not
    )
}

/// The index just after the type that starts at token `i`: `int`, `a.B<c, d>?`, `(int, string)`,
/// `fun(int) -> string?`.
fn skip_type(real: &[&Token], i: usize) -> usize {
    let tok = |j: usize| real.get(j).map(|t| &t.tok);
    let mut j = i;
    match tok(j) {
        Some(Tok::Fun) => {
            j += 1;
            if matches!(tok(j), Some(Tok::LParen)) {
                j += 1;
                while j < real.len() && !matches!(tok(j), Some(Tok::RParen)) {
                    j = skip_type(real, j);
                    if matches!(tok(j), Some(Tok::Comma)) {
                        j += 1;
                    } else if !matches!(tok(j), Some(Tok::RParen)) {
                        return j;
                    }
                }
                j += 1;
            }
            if matches!(tok(j), Some(Tok::Arrow)) {
                j = skip_type(real, j + 1);
            }
            j
        }
        Some(Tok::LParen) => {
            j += 1;
            while j < real.len() && !matches!(tok(j), Some(Tok::RParen)) {
                j = skip_type(real, j);
                if matches!(tok(j), Some(Tok::Comma)) {
                    j += 1;
                } else if !matches!(tok(j), Some(Tok::RParen)) {
                    return j;
                }
            }
            j += 1;
            if matches!(tok(j), Some(Tok::Question)) {
                j += 1;
            }
            j
        }
        Some(Tok::Ident(_)) | Some(Tok::None) => {
            j += 1;
            while matches!(tok(j), Some(Tok::Dot)) && matches!(tok(j + 1), Some(Tok::Ident(_))) {
                j += 2;
            }
            if matches!(tok(j), Some(Tok::Lt)) {
                j += 1;
                loop {
                    j = skip_type(real, j);
                    match tok(j) {
                        Some(Tok::Comma) => j += 1,
                        Some(Tok::Gt) => {
                            j += 1;
                            break;
                        }
                        _ => return j,
                    }
                }
            }
            if matches!(tok(j), Some(Tok::Question)) {
                j += 1;
            }
            j
        }
        _ => i,
    }
}

#[cfg(test)]
mod tests {
    use super::{format, typing};

    fn same(src: &str) {
        assert_eq!(format(src).unwrap(), src, "should be left as it is");
    }

    #[test]
    fn blocks_are_indented_by_four_spaces() {
        let messy = "fun f(x: int) -> int\nif x > 0 then\nreturn 1\nelseif x < 0 then\nreturn -1\nelse\nreturn 0\nend\nend\n";
        let tidy = "fun f(x: int) -> int\n    if x > 0 then\n        return 1\n    elseif x < 0 then\n        return -1\n    else\n        return 0\n    end\nend\n";
        assert_eq!(format(messy).unwrap(), tidy);
        same(tidy);
    }

    #[test]
    fn match_arms_are_one_level_in_and_their_bodies_one_more() {
        let src = "fun f(n: int) -> string\n    match n\n        case 0 then \"zero\"\n        case x if x > 9 then\n            let s = \"big\"\n            s\n        else \"other\"\n    end\nend\n";
        same(src);
        assert_eq!(
            format(&src.replace("\n        ", "\n").replace("\n    ", "\n")).unwrap(),
            src
        );
    }

    #[test]
    fn a_function_passed_as_an_argument_is_one_level_not_two() {
        same("queue.handle<Mail>(fun(m: Mail) -> string?\n    send(m)\n    none\nend)\n");
        same("let r = items.map(fun(x: int) -> int\n    x + 1\nend)\n");
    }

    #[test]
    fn function_types_are_not_blocks() {
        same(
            "struct Task\n    public run: fun() -> string?\n    public other: array<(string, fun(int) -> int)>\nend\n\ntype Handler = fun(Request) -> Response\n\nfun apply(f: fun(int) -> int, x: int) -> int\n    f(x)\nend\n",
        );
        same("let found = Reflect.find<fun(Request) -> Response>(name)\nlet again = 1\n");
    }

    #[test]
    fn traits_have_signatures_without_end_and_impl_for_is_one_block() {
        same(
            "trait Store\n    fun read(self, key: string) -> string?\n    fun write(self, key: string)\nend\n\nimpl Store for Memory\n    public fun read(self, key: string) -> string?\n        none\n    end\nend\n",
        );
    }

    #[test]
    fn brackets_hang_one_level() {
        same(
            "let row = JobRow {\n    id: 1,\n    kind: \"a\",\n}\nlet xs = [\n    1,\n    2,\n]\nfoo(\n    a,\n    b\n)\n",
        );
    }

    #[test]
    fn comments_follow_the_code_and_docs_and_strings_are_not_touched() {
        let src = "--\" A doc\n  with its own indentation\n--\"\nfun f() -> string\n    -- before a statement\n    let s = \"line one\n   line two\"\n    if true then\n        s\n        -- last of the block\n    end\n    s\nend\n";
        same(src);
    }

    #[test]
    fn blank_lines_collapse_and_trailing_space_goes() {
        assert_eq!(
            format("\n\nlet a = 1   \n\n\n\nlet b = 2\n\n\n").unwrap(),
            "let a = 1\n\nlet b = 2\n"
        );
        assert_eq!(format("").unwrap(), "");
        assert_eq!(format("  \n\n").unwrap(), "");
    }

    #[test]
    fn a_line_that_goes_on_from_an_operator_is_one_level_deeper() {
        same("let total = a +\n    b +\n    c\nlet ok = x\n    and y\n");
    }

    #[test]
    fn line_endings_are_kept() {
        let out = format("fun f()\r\nlet a = 1\r\nend\r\n").unwrap();
        assert_eq!(out, "fun f()\r\n    let a = 1\r\nend\r\n");
    }

    #[test]
    fn a_lone_carriage_return_before_the_final_line_break_is_stable() {
        let once = format("-- a\nfor i in 0(\"F\n\"x \r").unwrap();
        assert_eq!(format(&once).unwrap(), once);
    }

    #[test]
    fn many_lone_carriage_returns_never_make_the_layout_unstable() {
        let src = format!("-- a\nlet s = \"x\n{}\r\n\ny \r\n\"\n", "\r".repeat(5));
        if let Ok(once) = format(&src) {
            assert_eq!(format(&once).unwrap(), once);
        }
    }

    #[test]
    fn the_input_found_by_the_fuzzer_is_stable_or_refused() {
        let src =
            String::from_utf8_lossy(include_bytes!("../tests/data/fmt_lone_cr.q")).into_owned();
        if let Ok(once) = format(&src) {
            assert_eq!(format(&once).unwrap(), once);
        }
    }

    #[test]
    fn the_spaces_inside_a_line_are_put_in_order() {
        let messy = "fun   f( )\n    print (xs . len ( ) , 1)\nlet  a =  [1,2 ,  3 ]\nend\n";
        let tidy = "fun f()\n    print(xs.len(), 1)\n    let a = [1, 2, 3]\nend\n";
        assert_eq!(format(messy).unwrap(), tidy);
        same(tidy);
    }

    #[test]
    fn keywords_keep_a_space_before_a_parenthesis_and_a_lambda_does_not() {
        same("if (a) and (b) then\n    x\nend\nlet g = fun(x: int) -> int x * 2 end\n");
        assert_eq!(
            format("if(a)  and  (b) then\nx\nend\nlet g = fun (x: int) -> int x end\n").unwrap(),
            "if(a) and (b) then\n    x\nend\nlet g = fun(x: int) -> int x end\n"
        );
    }

    #[test]
    fn a_space_is_never_added_except_after_a_comma() {
        same("let a = x+1\nlet p = P { x:1 }\n");
        assert_eq!(format("f(a,b,)\n").unwrap(), "f(a, b,)\n");
    }

    #[test]
    fn strings_and_comments_keep_their_spaces() {
        same("let s = \"a   b ( )\" -- keep   ( )  this\n");
        assert_eq!(
            format("print(  \"a   b\"  )   -- x   y\n").unwrap(),
            "print(\"a   b\")   -- x   y\n"
        );
    }

    #[test]
    fn a_program_that_does_not_lex_is_refused() {
        assert!(format("let s = \"never closed\n").is_err());
    }

    #[test]
    fn formatting_twice_changes_nothing() {
        let src = "struct A\npublic x: int\nend\nimpl A\npublic fun f(self) -> int\nlet go = fun(y: int) -> int y + self.x end\nmatch self.x\ncase 1 then 2\nelse\nreturn go(3)\nend\nend\nend\n";
        let once = format(src).unwrap();
        assert_eq!(format(&once).unwrap(), once);
    }

    #[test]
    fn typing_puts_a_case_inside_its_match() {
        let src = "fun f(x: int) -> string\n    match x\ncase 1 then \"a\"\n    end\nend\n";
        assert_eq!(typing(src, 2).unwrap().level, 2);
        let src = "fun f(x: int)\n    match x\n        case 1 then\n            print(1)\ncase 2 then 3\n    end\nend\n";
        assert_eq!(typing(src, 4).unwrap().level, 2);
    }

    #[test]
    fn typing_gives_a_level_to_an_empty_line() {
        let src = "fun f()\n\n";
        assert_eq!(typing(src, 1).unwrap().level, 1);
        assert!(typing(src, 0).unwrap().unclosed);
    }

    #[test]
    fn typing_knows_if_the_block_is_closed() {
        assert!(typing("if x then\n\n", 0).unwrap().unclosed);
        assert!(!typing("if x then\n\nend\n", 0).unwrap().unclosed);
        assert!(!typing("let a = f(\n\n", 0).unwrap().unclosed);
        // a new `if` takes the `end` of the function around it: one is missing
        assert!(
            typing("fun f()\n    if x then\n\nend\n", 1)
                .unwrap()
                .unclosed
        );
        assert!(!typing("fun f()\n    let a = 1\nend\n", 1).unwrap().unclosed);
    }
}
