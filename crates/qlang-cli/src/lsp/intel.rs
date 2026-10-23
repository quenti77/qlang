//! What the editor shows, computed from what the checker learned (`Info`): hovers,
//! completion and signature help. Lines are `Vec<char>` and columns count characters, like
//! the compiler's locations (1-based there, 0-based here).

use qlang_core::check::{Info, Member};
use qlang_core::span::Location;
use std::collections::HashSet;

/// Documentation of the language's keywords.
pub const KEYWORDS: &[(&str, &str)] = &[
    (
        "fun",
        "Declares a function: `fun name(a: int) -> int ... end`.",
    ),
    (
        "struct",
        "Declares a structure: named fields, optionally `extends` another structure.",
    ),
    (
        "impl",
        "Gives methods to a type (`impl Point ... end`) or implements a trait (`impl Trait for Point`).",
    ),
    (
        "trait",
        "Declares a set of method signatures that types can implement.",
    ),
    ("enum", "Declares a type with a fixed list of named values."),
    ("let", "Declares a variable: `let x = 1`."),
    (
        "const",
        "Declares a constant name: it cannot be assigned again (the value it holds can still change).",
    ),
    (
        "if",
        "`if condition then ... elseif condition then ... else ... end`. Conditions must be `bool`.",
    ),
    ("then", "Starts the body of an `if` or `elseif`."),
    ("elseif", "Another condition of an `if`."),
    ("else", "What an `if` does when no condition is true."),
    ("while", "`while condition do ... end`."),
    (
        "for",
        "`for i in 0..10 do ... end` (`..=` includes the end), or `for k, v in map do ... end`.",
    ),
    ("in", "Used by `for ... in ...`."),
    ("step", "`for i in 0..10 step 2 do ... end`."),
    ("do", "Starts the body of a loop."),
    ("match", "`match value case A then ... else ... end`."),
    ("case", "One branch of a `match`."),
    ("return", "Leaves the function, optionally with a value."),
    ("break", "Leaves the loop."),
    ("continue", "Goes to the next turn of the loop."),
    ("end", "Closes a block."),
    (
        "import",
        "`import name from \"file.q\"` or `import \"file.q\" as module`.",
    ),
    ("export", "Makes names usable from other files."),
    (
        "extern",
        "Declares a function the host environment provides: `extern fun name(params) -> type`.",
    ),
    ("from", "Used by `import ... from \"file.q\"`."),
    ("as", "Converts a value (`3 as float`) or names an import."),
    ("and", "Logical and (both sides must be `bool`)."),
    ("or", "Logical or (both sides must be `bool`)."),
    ("not", "Logical not."),
    ("div", "Integer division: `7 div 2` is 3."),
    ("mod", "Remainder: `7 mod 2` is 1."),
    ("self", "The value a method is called on."),
    ("super", "The parent structure: `super.method()`."),
    ("public", "Visible everywhere (the default is `private`)."),
    ("private", "Visible only inside the `impl` of the type."),
    (
        "protected",
        "Visible in the type and the types that extend it.",
    ),
    ("static", "Belongs to the type, not to a value."),
    ("override", "Replaces a method of the parent structure."),
    ("extends", "Makes a structure inherit from another one."),
    ("true", "The boolean `true`."),
    ("false", "The boolean `false`."),
    ("none", "The absence of a value (`T?` types)."),
];

pub fn keyword_doc(word: &str) -> Option<&'static str> {
    KEYWORDS.iter().find(|(k, _)| *k == word).map(|(_, d)| *d)
}

pub fn fence(code: &str) -> String {
    format!("```qlang\n{code}\n```")
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The identifier around column `col`: (word, start, end), 0-based, end excluded.
pub fn word_at(line: &[char], col: usize) -> Option<(String, usize, usize)> {
    let mut s = col.min(line.len());
    let mut e = s;
    while s > 0 && is_word_char(line[s - 1]) {
        s -= 1;
    }
    while e < line.len() && is_word_char(line[e]) {
        e += 1;
    }
    if s == e || line[s].is_ascii_digit() {
        return None;
    }
    Some((line[s..e].iter().collect(), s, e))
}

/// The location spans the characters `start..end` of the (1-based) line.
pub fn same_range(loc: &Location, line1: u32, start: usize, end: usize) -> bool {
    loc.line == line1
        && loc.end_line == line1
        && loc.col as usize == start + 1
        && loc.end_col as usize == end + 1
}

/// The (1-based) position is inside the location.
pub fn within(loc: &Location, line1: u32, col1: u32) -> bool {
    if line1 < loc.line || line1 > loc.end_line {
        return false;
    }
    if line1 == loc.line && col1 < loc.col {
        return false;
    }
    if line1 == loc.end_line && col1 >= loc.end_col {
        return false;
    }
    true
}

fn before(loc: &Location, line1: u32, col1: u32) -> bool {
    loc.line < line1 || (loc.line == line1 && loc.col <= col1)
}

/// The column of the `.` that precedes the word starting at `word_start`.
pub fn dot_before(line: &[char], word_start: usize) -> Option<usize> {
    let mut i = word_start as isize - 1;
    while i >= 0 && (line[i as usize] == ' ' || line[i as usize] == '\t') {
        i -= 1;
    }
    if i >= 0 && line[i as usize] == '.' && (i == 0 || line[i as usize - 1] != '.') {
        Some(i as usize)
    } else {
        None
    }
}

/// The longest expression that ends right before the dot at (`line0`, `dot_col`).
fn receiver_at<'a>(
    info: &'a Info,
    file: &str,
    line0: usize,
    dot_col: usize,
) -> Option<&'a qlang_core::check::ExprInfo> {
    let mut best: Option<&qlang_core::check::ExprInfo> = None;
    for e in &info.exprs {
        let l = &e.loc;
        if l.file != file || l.end_line as usize != line0 + 1 || l.end_col as usize != dot_col + 1 {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => l.line < b.loc.line || (l.line == b.loc.line && l.col < b.loc.col),
        };
        if better {
            best = Some(e);
        }
    }
    best
}

fn members_of<'a>(info: &'a Info, expr: Option<&qlang_core::check::ExprInfo>) -> &'a [Member] {
    let Some(e) = expr else {
        return &[];
    };
    let found = match &e.statics {
        Some(key) => info.statics.get(key),
        None => info.members.get(&e.ty),
    };
    found.map(|v| v.as_slice()).unwrap_or(&[])
}

pub struct Hover {
    pub markdown: String,
    pub start: usize,
    pub end: usize,
}

/// What to show when hovering (`line0`, `col`) of `file`.
pub fn hover_at(info: &Info, file: &str, line0: usize, col: usize, text: &[char]) -> Option<Hover> {
    let (word, start, end) = word_at(text, col)?;
    let line1 = line0 as u32 + 1;
    let result = |code: &str, doc: Option<&str>| {
        Some(Hover {
            markdown: match doc {
                Some(d) if !d.is_empty() => format!("{}\n\n{d}", fence(code)),
                _ => fence(code),
            },
            start,
            end,
        })
    };

    // a declaration
    for l in &info.locals {
        if l.loc.file == file && same_range(&l.loc, line1, start, end) {
            return result(&format!("{} {}: {}", l.kind, l.name, l.ty), None);
        }
    }
    for it in &info.items {
        if it.loc.file == file && same_range(&it.loc, line1, start, end) {
            return result(&it.label, it.doc.as_deref());
        }
    }

    // `value.member` and `Type.member`
    if let Some(dot) = dot_before(text, start) {
        let member = members_of(info, receiver_at(info, file, line0, dot))
            .iter()
            .find(|m| m.name == word)?;
        return result(
            &format!("({}) {}", member.kind, member.detail),
            member.doc.as_deref(),
        );
    }

    // a name used in an expression
    let kinds = ["function", "struct", "enum", "trait"];
    let entry = info
        .exprs
        .iter()
        .find(|e| e.loc.file == file && same_range(&e.loc, line1, start, end));
    if let Some(e) = entry {
        let fallback = format!("{word}: {}", e.ty);
        return result(e.label.as_deref().unwrap_or(&fallback), e.doc.as_deref());
    }
    let rest: String = text[end..].iter().collect();
    let calls = rest.trim_start().starts_with('(');
    let site = info
        .items
        .iter()
        .find(|it| it.name == word && kinds.contains(&it.kind) && it.loc.file == file)
        .or_else(|| {
            info.items
                .iter()
                .find(|it| it.name == word && kinds.contains(&it.kind))
        });
    if let Some(s) = site {
        return result(&s.label, s.doc.as_deref());
    }
    if calls && let Some(b) = info.functions.iter().find(|f| f.name == word) {
        return result(&b.detail, b.doc.as_deref());
    }
    let local = info.locals.iter().find(|l| {
        l.name == word && l.loc.file == file && within(&l.scope, line1, start as u32 + 1)
    });
    if let Some(l) = local {
        return result(&format!("{} {}: {}", l.kind, l.name, l.ty), None);
    }
    if let Some(doc) = keyword_doc(&word) {
        return Some(Hover {
            markdown: format!("**{word}**: {doc}"),
            start,
            end,
        });
    }
    None
}

pub struct Item {
    pub label: String,
    pub kind: String,
    pub detail: String,
    pub doc: Option<String>,
    pub sort: String,
}

fn member_item(m: &Member, sort_prefix: &str) -> Item {
    Item {
        label: m.name.clone(),
        kind: m.kind.to_string(),
        detail: m.detail.clone(),
        doc: m.doc.clone(),
        sort: format!("{sort_prefix}{}", m.name),
    }
}

/// The completion items after a `.`: members of the receiver.
pub fn member_completions(
    info: &Info,
    file: &str,
    line0: usize,
    dot: usize,
    before_dot: &str,
) -> Vec<Item> {
    let show_private = {
        let t = before_dot.trim_end();
        t.ends_with("self")
            && !t[..t.len() - 4]
                .chars()
                .last()
                .is_some_and(|c| is_word_char(c) || c == '.')
    };
    members_of(info, receiver_at(info, file, line0, dot))
        .iter()
        .filter(|m| m.public || show_private)
        .map(|m| member_item(m, if m.kind == "field" { "0" } else { "1" }))
        .collect()
}

/// The completion items without a dot: variables in scope, names of the file, built-ins.
pub fn name_completions(info: &Info, file: &str, line0: usize, col: usize) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add = |item: Item| {
        if seen.insert(item.label.clone()) {
            out.push(item);
        }
    };
    let (line1, col1) = (line0 as u32 + 1, col as u32 + 1);
    for l in &info.locals {
        if l.loc.file != file || !before(&l.loc, line1, col1) || !within(&l.scope, line1, col1) {
            continue;
        }
        add(Item {
            label: l.name.clone(),
            kind: if l.kind == "param" {
                "param"
            } else {
                "variable"
            }
            .to_string(),
            detail: format!("{} {}: {}", l.kind, l.name, l.ty),
            doc: None,
            sort: format!("0{}", l.name),
        });
    }
    if let Some(globals) = info.globals.get(file) {
        for g in globals {
            add(member_item(g, "1"));
        }
    }
    for f in &info.functions {
        add(Item {
            label: f.name.clone(),
            kind: "function".to_string(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            sort: format!("2{}", f.name),
        });
    }
    for (k, doc) in KEYWORDS {
        add(Item {
            label: (*k).to_string(),
            kind: "keyword".to_string(),
            detail: "keyword".to_string(),
            doc: Some((*doc).to_string()),
            sort: format!("3{k}"),
        });
    }
    for t in [
        "int", "float", "bool", "string", "uuid", "ulid", "datetime", "bytes", "array", "map",
    ] {
        add(Item {
            label: t.to_string(),
            kind: "type".to_string(),
            detail: "type".to_string(),
            doc: None,
            sort: format!("4{t}"),
        });
    }
    for (name, doc) in [
        ("time", "The clock: `time.now()`."),
        (
            "random",
            "Random numbers: `random.int(1, 6)`, `random.float()`.",
        ),
        (
            "http",
            "HTTP requests: `http.send(Request { method: \"GET\", url: \"...\" })`.",
        ),
        (
            "regex",
            "Regular expressions: `regex.matches(text, pattern)`, `regex.find(text, pattern)`...",
        ),
    ] {
        add(Item {
            label: name.to_string(),
            kind: "module".to_string(),
            detail: "namespace".to_string(),
            doc: Some(doc.to_string()),
            sort: format!("4{name}"),
        });
    }
    out
}

pub struct StructLit {
    pub name: String,
    pub given: HashSet<String>,
    pub prefix: String,
}

/// The field name already given by the segment (`name: value`, or the shorthand `name`).
fn given_field(seg: &str) -> Option<String> {
    let t = seg.trim_start();
    let name: String = t.chars().take_while(|c| is_word_char(*c)).collect();
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let rest = t[name.len()..].trim_start();
    (rest.is_empty() || rest.starts_with(':')).then_some(name)
}

/// The struct literal (`Point { x: 1, | }`) around `offset` (in characters) of `text`, when
/// the cursor sits where a field name goes.
pub fn struct_lit_at(text: &[char], offset: usize) -> Option<StructLit> {
    struct Frame {
        brace: bool,
        name: Option<String>,
        given: HashSet<String>,
        seg_start: usize,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut in_string = false;
    let mut i = 0;
    while i < offset.min(text.len()) {
        let c = text[i];
        if in_string {
            if c == '\\' {
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => {
                let mut j = i as isize - 1;
                while j >= 0 && text[j as usize].is_whitespace() {
                    j -= 1;
                }
                let mut name = None;
                if j >= 0 && is_word_char(text[j as usize]) {
                    let mut k = j;
                    while k >= 0 && is_word_char(text[k as usize]) {
                        k -= 1;
                    }
                    let word: String = text[(k + 1) as usize..=(j as usize)].iter().collect();
                    if !word.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && keyword_doc(&word).is_none()
                    {
                        name = Some(word);
                    }
                }
                stack.push(Frame {
                    brace: true,
                    name,
                    given: HashSet::new(),
                    seg_start: i + 1,
                });
            }
            '(' | '[' => stack.push(Frame {
                brace: false,
                name: None,
                given: HashSet::new(),
                seg_start: 0,
            }),
            '}' | ')' | ']' => {
                stack.pop();
            }
            ',' => {
                if let Some(top) = stack.last_mut()
                    && top.brace
                {
                    let seg: String = text[top.seg_start..i].iter().collect();
                    if let Some(g) = given_field(&seg) {
                        top.given.insert(g);
                    }
                    top.seg_start = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let top = stack.pop()?;
    if !top.brace {
        return None;
    }
    let name = top.name?;
    let seg: String = text[top.seg_start..offset.min(text.len())].iter().collect();
    let t = seg.trim();
    if !t.chars().all(is_word_char) {
        return None;
    }
    Some(StructLit {
        name,
        given: top.given,
        prefix: t.to_string(),
    })
}

/// The completion items for the fields of a struct literal, excluding those already given.
pub fn struct_field_completions(info: &Info, lit: &StructLit) -> Vec<Item> {
    info.members
        .get(&lit.name)
        .map(|ms| {
            ms.iter()
                .filter(|m| m.kind == "field" && !lit.given.contains(&m.name))
                .map(|m| member_item(m, "0"))
                .collect()
        })
        .unwrap_or_default()
}

/// The dot that starts the member being typed at `col`: (dot column, start of the word).
pub fn member_access_at(line: &[char], col: usize) -> Option<(usize, usize)> {
    let mut s = col.min(line.len());
    while s > 0 && is_word_char(line[s - 1]) {
        s -= 1;
    }
    let mut d = s as isize - 1;
    while d >= 0 && (line[d as usize] == ' ' || line[d as usize] == '\t') {
        d -= 1;
    }
    if d >= 0 && line[d as usize] == '.' && (d == 0 || line[d as usize - 1] != '.') {
        Some((d as usize, s))
    } else {
        None
    }
}

pub struct Call {
    pub open: usize,
    pub name: String,
    pub active: usize,
}

/// The unfinished call around the cursor.
pub fn call_at(line: &[char], col: usize) -> Option<Call> {
    let mut in_string = false;
    let mut stack: Vec<(Option<usize>, usize)> = Vec::new();
    let mut i = 0;
    while i < col.min(line.len()) {
        let c = line[i];
        if in_string {
            if c == '\\' {
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => stack.push((Some(i), 0)),
            '[' | '{' => stack.push((None, 0)),
            ')' | ']' | '}' => {
                stack.pop();
            }
            ',' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (open, commas) = stack.pop()?;
    let open = open?;
    let mut e = open;
    while e > 0 && line[e - 1].is_whitespace() {
        e -= 1;
    }
    let mut s = e;
    while s > 0 && is_word_char(line[s - 1]) {
        s -= 1;
    }
    if s == e || line[s].is_ascii_digit() {
        return None;
    }
    Some(Call {
        open,
        name: line[s..e].iter().collect(),
        active: commas,
    })
}

/// `name(a: int, b: int) -> int` into its parameter labels.
pub fn parameters(detail: &str) -> Vec<String> {
    let chars: Vec<char> = detail.chars().collect();
    let Some(open) = chars.iter().position(|c| *c == '(') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut start = open + 1;
    let mut out = Vec::new();
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        if c == '(' || c == '<' {
            depth += 1;
        } else if c == ')' || c == '>' {
            if c == ')' && depth == 1 {
                let last: String = chars[start..i].iter().collect();
                if !last.trim().is_empty() {
                    out.push(last.trim().to_string());
                }
                return out;
            }
            if !(c == '>' && i > 0 && chars[i - 1] == '-') {
                depth -= 1;
            }
        } else if c == ',' && depth == 1 {
            let part: String = chars[start..i].iter().collect();
            out.push(part.trim().to_string());
            start = i + 1;
        }
        i += 1;
    }
    out
}

pub struct Signature {
    pub label: String,
    pub parameters: Vec<String>,
    pub doc: Option<String>,
    pub active: usize,
}

/// The signature of what `call` calls, if known.
pub fn signature_for(
    info: &Info,
    file: &str,
    line0: usize,
    text: &[char],
    call: &Call,
) -> Option<Signature> {
    let mut name_end = call.open;
    while name_end > 0 && text[name_end - 1].is_whitespace() {
        name_end -= 1;
    }
    let name_start = name_end - call.name.chars().count();
    let (label, doc) = if let Some(dot) = dot_before(text, name_start) {
        let m = members_of(info, receiver_at(info, file, line0, dot))
            .iter()
            .find(|m| m.name == call.name)?;
        (m.detail.clone(), m.doc.clone())
    } else {
        let site = info
            .items
            .iter()
            .find(|it| it.name == call.name && it.kind == "function");
        let builtin = info.functions.iter().find(|f| f.name == call.name);
        match (site, builtin) {
            (Some(s), _) => (
                s.label.strip_prefix("fun ").unwrap_or(&s.label).to_string(),
                s.doc.clone(),
            ),
            (None, Some(b)) => (b.detail.clone(), b.doc.clone()),
            _ => return None,
        }
    };
    Some(Signature {
        parameters: parameters(&label),
        label,
        doc,
        active: call.active,
    })
}

/// Replace the characters `from..to` of line `line0` by spaces: to ask the compiler about
/// `value.partial|` or `f(arg|` as if the unfinished part was not there.
pub fn blank(text: &str, line0: usize, from: usize, to: usize) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if let Some(l) = lines.get_mut(line0) {
        let chars: Vec<char> = l.chars().collect();
        let to = to.min(chars.len());
        let from = from.min(to);
        let mut out: String = chars[..from].iter().collect();
        out.extend(std::iter::repeat_n(' ', to - from));
        out.extend(chars[to..].iter());
        *l = out;
    }
    lines.join("\n")
}
