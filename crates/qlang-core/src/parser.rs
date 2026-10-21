//! Parser: tokens to syntax tree. Hand-written recursive descent that
//! follows `qlang.g4`, with error recovery so several errors can be reported.
//!
//! Line breaks end statements. They are ignored inside `()`, `[]` and `{}`
//! and after operators or commas; `nl` is a stack telling whether line
//! breaks are significant in the construct being parsed (blocks push `true`,
//! delimiters push `false`).

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::lexer::{Tok, Token, lex, lex_with_docs};
use crate::span::Span;
use std::rc::Rc;

type PResult<T> = Result<T, ()>;

/// Whether a `fun` declaration being parsed must, or may, have a body.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyMode {
    Required,
    /// Only for trait method signatures: a body makes it a default implementation.
    Optional,
    /// `extern fun`: never has a body.
    Absent,
}

/// Deepest nesting of expressions, blocks and types the parser accepts.
const MAX_NESTING: u32 = 200;

pub struct Parser<'a> {
    src: &'a str,
    file: u32,
    toks: Vec<Token>,
    pos: usize,
    nl: Vec<bool>,
    next_id: u32,
    /// Documentation comments not attached yet, by the line where they end: (end offset, text).
    docs: std::collections::HashMap<usize, (usize, Rc<str>)>,
    line_starts: Vec<usize>,
    /// Current nesting of expressions, blocks and types (bounded).
    depth: u32,
    pub diags: Vec<Diagnostic>,
}

/// Parse a whole file. Node ids start at 0.
pub fn parse_module(src: &str, file: u32, diags: &mut Vec<Diagnostic>) -> Module {
    parse_module_from(src, file, 0, diags).0
}

/// Parse a whole file with node ids starting at `first_id`, so that several
/// files can share one id space. Returns the module and the next free id.
pub fn parse_module_from(
    src: &str,
    file: u32,
    first_id: u32,
    diags: &mut Vec<Diagnostic>,
) -> (Module, u32) {
    let mut docs = Vec::new();
    let toks = lex_with_docs(src, file, 0, diags, &mut docs);
    let mut p = Parser::new(src, file, toks, first_id);
    for d in docs {
        let line = p.line_of(d.end.saturating_sub(1));
        p.docs.insert(line, (d.end, Rc::from(d.text.as_str())));
    }
    let m = p.parse_items();
    diags.append(&mut p.diags);
    (m, p.next_id)
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, file: u32, toks: Vec<Token>, next_id: u32) -> Parser<'a> {
        Parser {
            src,
            file,
            toks,
            pos: 0,
            nl: vec![true],
            next_id,
            docs: std::collections::HashMap::new(),
            line_starts: std::iter::once(0)
                .chain(src.match_indices('\n').map(|(i, _)| i + 1))
                .collect(),
            depth: 0,
            diags: Vec::new(),
        }
    }

    // ------------------------------------------------------------------ docs

    fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset) - 1
    }

    /// The documentation comment that ends on the line above the declaration starting at
    /// `start` (or earlier on its own line), if there is one. A comment documents one declaration.
    fn take_doc(&mut self, start: Span) -> Option<Rc<str>> {
        let line = self.line_of(start.start as usize);
        for key in [line.wrapping_sub(1), line] {
            if let Some((end, _)) = self.docs.get(&key)
                && *end <= start.start as usize
            {
                return self.docs.remove(&key).map(|(_, text)| text);
            }
        }
        None
    }

    // ---------------------------------------------------------------- tokens

    fn skip_insignificant(&mut self) {
        if !*self.nl.last().unwrap() {
            while self.toks[self.pos].tok == Tok::Newline {
                self.pos += 1;
            }
        }
    }

    fn peek(&mut self) -> Tok {
        self.skip_insignificant();
        self.toks[self.pos].tok.clone()
    }

    fn peek_span(&mut self) -> Span {
        self.skip_insignificant();
        self.toks[self.pos].span
    }

    /// The token after the current one, without consuming anything.
    fn peek2(&mut self) -> Tok {
        self.skip_insignificant();
        let mut i = self.pos + 1;
        if !*self.nl.last().unwrap() {
            while self.toks[i].tok == Tok::Newline {
                i += 1;
            }
        }
        self.toks[i].tok.clone()
    }

    /// The first token that isn't a line break, from the current position on
    /// (inclusive), without consuming anything: a non-destructive lookahead
    /// past blank lines, for deciding whether a declaration continues.
    fn peek_past_newlines(&mut self) -> Tok {
        self.skip_insignificant();
        let mut i = self.pos;
        while self.toks[i].tok == Tok::Newline {
            i += 1;
        }
        self.toks[i].tok.clone()
    }

    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            self.toks[0].span
        } else {
            self.toks[self.pos - 1].span
        }
    }

    fn bump(&mut self) -> Token {
        self.skip_insignificant();
        let t = self.toks[self.pos].clone();
        if t.tok != Tok::Eof {
            self.pos += 1;
        }
        t
    }

    fn at(&mut self, t: &Tok) -> bool {
        self.peek() == *t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume line breaks no matter the mode.
    fn skip_newlines(&mut self) {
        while self.toks[self.pos].tok == Tok::Newline {
            self.pos += 1;
        }
    }

    fn error_at<T>(&mut self, code: &str, msg: impl Into<String>, span: Span) -> PResult<T> {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span));
        Err(())
    }

    fn expect(&mut self, t: &Tok) -> PResult<Span> {
        if self.at(t) {
            Ok(self.bump().span)
        } else {
            self.unexpected(&format!("`{}`", t.text()))
        }
    }

    fn unexpected<T>(&mut self, expected: &str) -> PResult<T> {
        let found = self.peek();
        let span = self.peek_span();
        self.error_at(
            "P001",
            format!("expected {expected}, found {}", found.describe()),
            span,
        )
    }

    fn expect_ident(&mut self) -> PResult<Ident> {
        match self.peek() {
            Tok::Ident(name) => {
                let span = self.bump().span;
                Ok(Ident { name, span })
            }
            _ => self.unexpected("a name"),
        }
    }

    fn mk(&mut self, span: Span, kind: ExprKind) -> Expr {
        let id = self.next_id;
        self.next_id += 1;
        Expr { id, span, kind }
    }

    /// Enter a nested construct; fails if the source is nested absurdly deep.
    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            let span = self.peek_span();
            self.depth -= 1;
            return self.error_at("P003", "this code is nested too deeply", span);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn fresh_id(&mut self) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // -------------------------------------------------------------- recovery

    /// After a statement: the line must end (or a closing keyword follows).
    fn end_of_stmt(&mut self, terms: &[Tok]) -> bool {
        let t = self.peek();
        if t == Tok::Newline || t == Tok::Eof || terms.contains(&t) {
            return true;
        }
        let span = self.peek_span();
        crate::diag::push_capped(
            &mut self.diags,
            Diagnostic::error(
                "P002",
                format!("unexpected {}; expected the end of the line", t.describe()),
                span,
            ),
        );
        false
    }

    fn sync(&mut self, terms: &[Tok]) {
        loop {
            let t = self.peek();
            if t == Tok::Newline || t == Tok::Eof || terms.contains(&t) {
                break;
            }
            self.bump();
        }
    }

    // ----------------------------------------------------------------- items

    fn parse_items(&mut self) -> Module {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::Eof) {
                break;
            }
            let before = self.pos;
            match self.parse_item() {
                Ok(item) => {
                    items.push(item);
                    if !self.end_of_stmt(&[]) {
                        self.sync(&[]);
                    }
                }
                Err(()) => self.sync(&[]),
            }
            if self.pos == before {
                self.bump();
            }
        }
        Module { items }
    }

    fn parse_item(&mut self) -> PResult<Item> {
        match self.peek() {
            Tok::Import => self.parse_import().map(Item::Import),
            Tok::Export => self.parse_export().map(Item::Export),
            Tok::Fun if matches!(self.toks[self.pos + 1].tok, Tok::Ident(_)) => self
                .parse_fun(true, BodyMode::Required)
                .map(|f| Item::Fun(Rc::new(f))),
            Tok::Extern => self.parse_extern().map(|f| Item::Fun(Rc::new(f))),
            Tok::Struct => self.parse_struct().map(Item::Struct),
            Tok::Impl => self.parse_impl().map(Item::Impl),
            Tok::Trait => self.parse_trait().map(Item::Trait),
            Tok::Enum => self.parse_enum().map(Item::Enum),
            Tok::Public | Tok::Private | Tok::Protected | Tok::Static | Tok::Override => {
                let span = self.peek_span();
                self.error_at(
                    "P050",
                    "visibility and `static` only apply to members of a `struct` or `impl`",
                    span,
                )
            }
            _ => self.parse_stmt().map(Item::Stmt),
        }
    }

    /// `extern fun name(params) -> ret`: a function the host environment provides.
    fn parse_extern(&mut self) -> PResult<FunDecl> {
        let start = self.expect(&Tok::Extern)?;
        if !(self.at(&Tok::Fun) && matches!(self.toks[self.pos + 1].tok, Tok::Ident(_))) {
            let span = self.peek_span();
            return self.error_at("P052", "`extern` must be followed by `fun name(...)`", span);
        }
        let mut f = self.parse_fun(true, BodyMode::Absent)?;
        f.span = start.to(f.span);
        Ok(f)
    }

    fn parse_import(&mut self) -> PResult<Import> {
        let start = self.expect(&Tok::Import)?;
        if let Tok::Str(raw) = self.peek() {
            let path_span = self.bump().span;
            self.expect(&Tok::As)?;
            let alias = self.expect_ident()?;
            return Ok(Import {
                span: start.to(alias.span),
                path: raw,
                path_span,
                kind: ImportKind::Module(alias),
            });
        }
        let mut names = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let alias = if self.eat(&Tok::As) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            names.push(ImportName { name, alias });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::From)?;
        match self.peek() {
            Tok::Str(raw) => {
                let path_span = self.bump().span;
                Ok(Import {
                    span: start.to(path_span),
                    path: raw,
                    path_span,
                    kind: ImportKind::Names(names),
                })
            }
            _ => self.unexpected("a file path in quotes"),
        }
    }

    fn parse_export(&mut self) -> PResult<Export> {
        let start = self.expect(&Tok::Export)?;
        let mut names = Vec::new();
        loop {
            names.push(self.expect_ident()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let end = names.last().unwrap().span;
        Ok(Export {
            span: start.to(end),
            names,
        })
    }

    fn parse_type_params(&mut self) -> PResult<Vec<TypeParam>> {
        self.expect(&Tok::Lt)?;
        let mut out = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let mut bounds = Vec::new();
            if self.eat(&Tok::Colon) {
                loop {
                    bounds.push(self.parse_type()?);
                    if !self.eat(&Tok::Plus) {
                        break;
                    }
                }
            }
            out.push(TypeParam { name, bounds });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::Gt)?;
        Ok(out)
    }

    /// `fun name<T>(params) -> ret` and, if `require_body`, its block and `end`.
    fn parse_fun(&mut self, named: bool, body: BodyMode) -> PResult<FunDecl> {
        let start = self.expect(&Tok::Fun)?;
        let name = if named {
            Some(self.expect_ident()?)
        } else {
            None
        };
        let tparams = if self.at(&Tok::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        self.expect(&Tok::LParen)?;
        self.nl.push(false);
        let mut has_self = false;
        let mut params = Vec::new();
        let r: PResult<()> = (|| {
            while !self.at(&Tok::RParen) {
                if self.at(&Tok::SelfKw) {
                    let span = self.bump().span;
                    if !params.is_empty() || has_self {
                        return self.error_at("P041", "`self` must be the first parameter", span);
                    }
                    has_self = true;
                } else {
                    let pname = self.expect_ident()?;
                    if !self.at(&Tok::Colon) {
                        let span = pname.span;
                        return self.error_at(
                            "P042",
                            format!(
                                "parameter `{}` needs a type (`{}: type`)",
                                pname.name, pname.name
                            ),
                            span,
                        );
                    }
                    self.bump();
                    let ty = self.parse_type()?;
                    params.push(Param { name: pname, ty });
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
            Ok(())
        })();
        self.nl.pop();
        r?;
        let ret = if self.eat(&Tok::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body_mode = body;
        let has_body = match body {
            BodyMode::Required => true,
            BodyMode::Absent => false,
            // a trait method has a default body unless its signature ends the line: the
            // next token is then `fun` (the next signature) or `end` (no statement can
            // start with a bare `fun` outside the top level, so this is unambiguous).
            BodyMode::Optional => {
                !matches!(self.peek_past_newlines(), Tok::Fun | Tok::End | Tok::Eof)
            }
        };
        let (body, end_span) = if has_body {
            let block = self.parse_block(&[Tok::End]);
            let end = self.expect(&Tok::End)?;
            (Some(block), end)
        } else {
            (None, self.prev_span())
        };
        let id = self.fresh_id();
        let doc = if named { self.take_doc(start) } else { None };
        Ok(FunDecl {
            doc,
            id,
            span: start.to(end_span),
            name,
            tparams,
            has_self,
            params,
            ret,
            body,
            external: matches!(body_mode, BodyMode::Absent),
        })
    }

    fn parse_visibility(&mut self) -> Option<Visibility> {
        match self.peek() {
            Tok::Public => {
                self.bump();
                Some(Visibility::Public)
            }
            Tok::Private => {
                self.bump();
                Some(Visibility::Private)
            }
            Tok::Protected => {
                self.bump();
                Some(Visibility::Protected)
            }
            _ => None,
        }
    }

    fn parse_struct(&mut self) -> PResult<StructDecl> {
        let start = self.expect(&Tok::Struct)?;
        let doc = self.take_doc(start);
        let name = self.expect_ident()?;
        let tparams = if self.at(&Tok::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        let parent = if self.eat(&Tok::Extends) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::End) || self.at(&Tok::Eof) {
                break;
            }
            let before = self.pos;
            match self.parse_field() {
                Ok(f) => {
                    fields.push(f);
                    if !self.end_of_stmt(&[Tok::End]) {
                        self.sync(&[Tok::End]);
                    }
                }
                Err(()) => self.sync(&[Tok::End]),
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.expect(&Tok::End)?;
        Ok(StructDecl {
            doc,
            span: start.to(end),
            name,
            tparams,
            parent,
            fields,
        })
    }

    fn parse_field(&mut self) -> PResult<FieldDecl> {
        let first = self.peek_span();
        let doc = self.take_doc(first);
        let vis = self.parse_visibility();
        let is_static = self.eat(&Tok::Static);
        let name = self.expect_ident()?;
        self.expect(&Tok::Colon)?;
        let ty = self.parse_type()?;
        let default = if self.eat(&Tok::Assign) {
            self.skip_newlines();
            Some(Rc::new(self.parse_expr()?))
        } else {
            None
        };
        Ok(FieldDecl {
            doc,
            vis,
            is_static,
            name,
            ty,
            default,
        })
    }

    fn parse_impl(&mut self) -> PResult<ImplDecl> {
        let start = self.expect(&Tok::Impl)?;
        let tparams = if self.at(&Tok::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        let first = self.parse_type()?;
        let (trait_ref, target) = if self.eat(&Tok::For) {
            let t = self.parse_type()?;
            (Some(first), t)
        } else {
            (None, first)
        };
        let mut methods = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::End) || self.at(&Tok::Eof) {
                break;
            }
            let before = self.pos;
            let r: PResult<MethodDecl> = (|| {
                let vis = self.parse_visibility();
                let is_override = self.eat(&Tok::Override);
                if !self.at(&Tok::Fun) {
                    return self.unexpected("a method (`fun name(...)`) or `end`");
                }
                let fun = Rc::new(self.parse_fun(true, BodyMode::Required)?);
                Ok(MethodDecl {
                    vis,
                    is_override,
                    fun,
                })
            })();
            match r {
                Ok(m) => {
                    methods.push(m);
                    if !self.end_of_stmt(&[Tok::End]) {
                        self.sync(&[Tok::End]);
                    }
                }
                Err(()) => self.sync(&[Tok::End]),
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.expect(&Tok::End)?;
        Ok(ImplDecl {
            span: start.to(end),
            tparams,
            trait_ref,
            target,
            methods,
        })
    }

    fn parse_trait(&mut self) -> PResult<TraitDecl> {
        let start = self.expect(&Tok::Trait)?;
        let doc = self.take_doc(start);
        let name = self.expect_ident()?;
        let tparams = if self.at(&Tok::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        let mut sigs = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::End) || self.at(&Tok::Eof) {
                break;
            }
            let before = self.pos;
            let r = if self.at(&Tok::Fun) {
                self.parse_fun(true, BodyMode::Optional)
            } else {
                self.unexpected("a method signature (`fun name(...)`) or `end`")
            };
            match r {
                Ok(s) => {
                    sigs.push(Rc::new(s));
                    if !self.end_of_stmt(&[Tok::End]) {
                        self.sync(&[Tok::End]);
                    }
                }
                Err(()) => self.sync(&[Tok::End]),
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.expect(&Tok::End)?;
        Ok(TraitDecl {
            doc,
            span: start.to(end),
            name,
            tparams,
            sigs,
        })
    }

    fn parse_enum(&mut self) -> PResult<EnumDecl> {
        let start = self.expect(&Tok::Enum)?;
        let doc = self.take_doc(start);
        let name = self.expect_ident()?;
        let tparams = if self.at(&Tok::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        let mut variants = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::End) || self.at(&Tok::Eof) {
                break;
            }
            let before = self.pos;
            let first = self.peek_span();
            let variant_doc = self.take_doc(first);
            match self.parse_variant(variant_doc) {
                Ok(v) => {
                    variants.push(v);
                    if !self.end_of_stmt(&[Tok::End]) {
                        self.sync(&[Tok::End]);
                    }
                }
                Err(()) => self.sync(&[Tok::End]),
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.expect(&Tok::End)?;
        Ok(EnumDecl {
            doc,
            span: start.to(end),
            name,
            tparams,
            variants,
        })
    }

    /// `Name` or `Name(field: type, ...)`.
    fn parse_variant(&mut self, doc: Option<Rc<str>>) -> PResult<VariantDecl> {
        let name = self.expect_ident()?;
        let mut fields = Vec::new();
        if self.at(&Tok::LParen) {
            self.bump();
            self.nl.push(false);
            let r: PResult<()> = (|| {
                while !self.at(&Tok::RParen) {
                    let fname = self.expect_ident()?;
                    if !self.at(&Tok::Colon) {
                        let span = fname.span;
                        return self.error_at(
                            "P042",
                            format!(
                                "field `{}` needs a type (`{}: type`)",
                                fname.name, fname.name
                            ),
                            span,
                        );
                    }
                    self.bump();
                    let ty = self.parse_type()?;
                    fields.push(Param { name: fname, ty });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen)?;
                Ok(())
            })();
            self.nl.pop();
            r?;
        }
        Ok(VariantDecl { name, doc, fields })
    }

    // ---------------------------------------------------------------- blocks

    fn parse_block(&mut self, terms: &[Tok]) -> Block {
        if self.enter().is_err() {
            self.sync(terms);
            return Block {
                span: self.peek_span(),
                stmts: Vec::new(),
            };
        }
        self.nl.push(true);
        let start = self.peek_span();
        let mut stmts: Vec<Stmt> = Vec::new();
        loop {
            self.skip_newlines();
            let t = self.peek();
            if t == Tok::Eof || terms.contains(&t) {
                break;
            }
            let before = self.pos;
            match self.parse_stmt() {
                Ok(s) => {
                    stmts.push(s);
                    if !self.end_of_stmt(terms) {
                        self.sync(terms);
                    }
                }
                Err(()) => self.sync(terms),
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.nl.pop();
        self.leave();
        let span = match stmts.last() {
            Some(last) => start.to(last.span),
            None => start,
        };
        Block { span, stmts }
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let start = self.peek_span();
        let kind = match self.peek() {
            Tok::Let | Tok::Const => self.parse_let()?,
            Tok::Return => {
                self.bump();
                let t = self.peek();
                let has_value = !matches!(
                    t,
                    Tok::Newline | Tok::Eof | Tok::End | Tok::Else | Tok::ElseIf | Tok::Case
                );
                StmtKind::Return(if has_value {
                    Some(self.parse_expr()?)
                } else {
                    None
                })
            }
            Tok::While => {
                self.bump();
                let cond = self.parse_expr()?;
                self.expect(&Tok::Do)?;
                let body = self.parse_block(&[Tok::End]);
                self.expect(&Tok::End)?;
                StmtKind::While { cond, body }
            }
            Tok::For => {
                let for_span = self.bump().span;
                // each loop variable is a name or a tuple pattern: `for (a, b) in pairs`
                let mut var_pattern = None;
                let var = if self.at(&Tok::LParen) {
                    var_pattern = Some(self.parse_tuple_pattern()?);
                    Ident {
                        name: String::new(),
                        span: for_span,
                    }
                } else {
                    self.expect_ident()?
                };
                let mut var2_pattern = None;
                let var2 = if self.eat(&Tok::Comma) {
                    if self.at(&Tok::LParen) {
                        let p = self.parse_tuple_pattern()?;
                        let span = p.span();
                        var2_pattern = Some(p);
                        Some(Ident {
                            name: String::new(),
                            span,
                        })
                    } else {
                        Some(self.expect_ident()?)
                    }
                } else {
                    None
                };
                self.expect(&Tok::In)?;
                let iter = self.parse_expr()?;
                let step = if self.eat(&Tok::Step) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(&Tok::Do)?;
                let body = self.parse_block(&[Tok::End]);
                self.expect(&Tok::End)?;
                StmtKind::For {
                    var,
                    var2,
                    var_pattern,
                    var2_pattern,
                    iter,
                    step,
                    body,
                }
            }
            Tok::Break => {
                self.bump();
                StmtKind::Break
            }
            Tok::Continue => {
                self.bump();
                StmtKind::Continue
            }
            Tok::Struct
            | Tok::Impl
            | Tok::Trait
            | Tok::Enum
            | Tok::Import
            | Tok::Export
            | Tok::Extern => {
                let t = self.peek();
                let span = self.peek_span();
                return self.error_at(
                    "P051",
                    format!("`{}` is only allowed at the top level of a file", t.text()),
                    span,
                );
            }
            Tok::Fun if matches!(self.toks[self.pos + 1].tok, Tok::Ident(_)) => {
                let span = self.peek_span();
                return self.error_at(
                    "P052",
                    "named functions are only allowed at the top level; here write `let name = fun(...) ... end`",
                    span,
                );
            }
            _ => StmtKind::Expr(self.parse_expr()?),
        };
        let end = self.prev_span();
        Ok(Stmt {
            span: start.to(end),
            kind,
        })
    }

    /// `(a, _, (b, c))`: two or more names, `_` or nested patterns.
    fn parse_tuple_pattern(&mut self) -> PResult<TuplePattern> {
        let start = self.expect(&Tok::LParen)?;
        self.nl.push(false);
        let mut items = Vec::new();
        let r: PResult<Span> = (|| {
            while !self.at(&Tok::RParen) {
                if self.at(&Tok::LParen) {
                    items.push(self.parse_tuple_pattern()?);
                } else {
                    let name = self.expect_ident()?;
                    items.push(if name.name == "_" {
                        TuplePattern::Wild(name.span)
                    } else {
                        TuplePattern::Name(name)
                    });
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)
        })();
        self.nl.pop();
        let end = r?;
        if items.len() < 2 {
            return self.error_at(
                "P061",
                "a tuple pattern needs at least two names: `(a, b)`",
                start.to(end),
            );
        }
        Ok(TuplePattern::Tuple(items, start.to(end)))
    }

    fn parse_let(&mut self) -> PResult<StmtKind> {
        let first = self.peek_span();
        let doc = self.take_doc(first);
        let is_const = self.bump().tok == Tok::Const;
        if self.at(&Tok::LParen) {
            // `let (a, b) = expr`
            let pattern = self.parse_tuple_pattern()?;
            let ty = if self.eat(&Tok::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            if !self.at(&Tok::Assign) {
                let span = self.peek_span();
                return self.error_at(
                    "P060",
                    "a tuple pattern needs a value: `let (a, b) = expr`",
                    span,
                );
            }
            self.bump();
            self.skip_newlines();
            let init = self.parse_expr()?;
            return Ok(StmtKind::LetTuple {
                is_const,
                pattern,
                ty,
                init,
            });
        }
        let name = self.expect_ident()?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        if !self.at(&Tok::Assign) {
            let span = self.peek_span();
            return self.error_at(
                "P060",
                format!(
                    "`{0}` has no initial value: variables must be initialized (write `let {0}: T? = none` for an empty variable)",
                    name.name
                ),
                span,
            );
        }
        self.bump();
        self.skip_newlines();
        let init = self.parse_expr()?;
        Ok(StmtKind::Let {
            doc,
            is_const,
            name,
            ty,
            init,
        })
    }

    // ----------------------------------------------------------------- types

    fn parse_type(&mut self) -> PResult<TypeExpr> {
        self.enter()?;
        let r = self.parse_type_inner();
        self.leave();
        r
    }

    fn parse_type_inner(&mut self) -> PResult<TypeExpr> {
        let start = self.peek_span();
        let base = match self.peek() {
            Tok::Fun => {
                self.bump();
                self.expect(&Tok::LParen)?;
                self.nl.push(false);
                let mut params = Vec::new();
                let r: PResult<()> = (|| {
                    while !self.at(&Tok::RParen) {
                        params.push(self.parse_type()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen)?;
                    Ok(())
                })();
                self.nl.pop();
                r?;
                let ret = if self.eat(&Tok::Arrow) {
                    Some(Box::new(self.parse_type()?))
                } else {
                    None
                };
                // a function type is never directly nullable: write (fun(..) -> ..)?
                return Ok(TypeExpr {
                    span: start.to(self.prev_span()),
                    kind: TypeExprKind::Fun { params, ret },
                });
            }
            Tok::LParen => {
                self.bump();
                self.nl.push(false);
                // `(T)` is just T; `(A, B)` is a tuple type
                let r: PResult<TypeExpr> = (|| {
                    let first = self.parse_type()?;
                    if !self.at(&Tok::Comma) {
                        self.expect(&Tok::RParen)?;
                        return Ok(first);
                    }
                    let mut items = vec![first];
                    while self.eat(&Tok::Comma) {
                        if self.at(&Tok::RParen) {
                            break;
                        }
                        items.push(self.parse_type()?);
                    }
                    let end = self.expect(&Tok::RParen)?;
                    if items.len() < 2 {
                        return self.error_at(
                            "P060",
                            "a tuple type needs at least two types: `(int, string)`",
                            start.to(end),
                        );
                    }
                    Ok(TypeExpr {
                        span: start.to(end),
                        kind: TypeExprKind::Tuple(items),
                    })
                })();
                self.nl.pop();
                r?
            }
            Tok::Ident(_) => {
                let mut path = vec![self.expect_ident()?];
                while self.at(&Tok::Dot) {
                    self.bump();
                    path.push(self.expect_ident()?);
                }
                let args = if self.at(&Tok::Lt) {
                    self.parse_type_args()?
                } else {
                    Vec::new()
                };
                TypeExpr {
                    span: start.to(self.prev_span()),
                    kind: TypeExprKind::Named { path, args },
                }
            }
            _ => return self.unexpected("a type"),
        };
        if self.at(&Tok::Question) {
            let q = self.bump().span;
            if self.at(&Tok::Question) {
                let span = self.peek_span();
                return self.error_at(
                    "P030",
                    "a type can only be nullable once (`int?`, not `int??`)",
                    span,
                );
            }
            return Ok(TypeExpr {
                span: start.to(q),
                kind: TypeExprKind::Nullable(Box::new(base)),
            });
        }
        Ok(base)
    }

    fn parse_type_args(&mut self) -> PResult<Vec<TypeExpr>> {
        self.expect(&Tok::Lt)?;
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::Gt)?;
        Ok(args)
    }

    // ----------------------------------------------------------- expressions

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.enter()?;
        let r = self.parse_assignment();
        self.leave();
        r
    }

    fn parse_assignment(&mut self) -> PResult<Expr> {
        let lhs = self.parse_range()?;
        let op = match self.peek() {
            Tok::Assign => Some(None),
            Tok::PlusAssign => Some(Some(BinOp::Add)),
            Tok::MinusAssign => Some(Some(BinOp::Sub)),
            Tok::StarAssign => Some(Some(BinOp::Mul)),
            Tok::SlashAssign => Some(Some(BinOp::Div)),
            Tok::PercentAssign | Tok::ModAssign => Some(Some(BinOp::Mod)),
            Tok::IntDivAssign => Some(Some(BinOp::IntDiv)),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            self.skip_newlines();
            let rhs = self.parse_assignment()?;
            let span = lhs.span.to(rhs.span);
            return Ok(self.mk(
                span,
                ExprKind::Assign {
                    op,
                    target: Box::new(lhs),
                    value: Box::new(rhs),
                },
            ));
        }
        Ok(lhs)
    }

    fn parse_range(&mut self) -> PResult<Expr> {
        let lhs = self.parse_or()?;
        let inclusive = match self.peek() {
            Tok::DotDot => false,
            Tok::DotDotEq => true,
            _ => return Ok(lhs),
        };
        self.bump();
        self.skip_newlines();
        let rhs = self.parse_or()?;
        let span = lhs.span.to(rhs.span);
        Ok(self.mk(
            span,
            ExprKind::Range {
                start: Box::new(lhs),
                end: Box::new(rhs),
                inclusive,
            },
        ))
    }

    fn binary(&mut self, op: BinOp, l: Expr, r: Expr) -> Expr {
        let span = l.span.to(r.span);
        self.mk(span, ExprKind::Binary(op, Box::new(l), Box::new(r)))
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut l = self.parse_and()?;
        while self.at(&Tok::Or) {
            self.bump();
            self.skip_newlines();
            let r = self.parse_and()?;
            l = self.binary(BinOp::Or, l, r);
        }
        Ok(l)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut l = self.parse_not()?;
        while self.at(&Tok::And) {
            self.bump();
            self.skip_newlines();
            let r = self.parse_not()?;
            l = self.binary(BinOp::And, l, r);
        }
        Ok(l)
    }

    fn parse_not(&mut self) -> PResult<Expr> {
        if self.at(&Tok::Not) {
            let start = self.bump().span;
            let e = self.parse_not()?;
            let span = start.to(e.span);
            return Ok(self.mk(span, ExprKind::Unary(UnOp::Not, Box::new(e))));
        }
        self.parse_comparison()
    }

    fn cmp_op(t: &Tok) -> Option<BinOp> {
        Some(match t {
            Tok::EqEq => BinOp::Eq,
            Tok::NotEq => BinOp::NotEq,
            Tok::Lt => BinOp::Lt,
            Tok::LtEq => BinOp::LtEq,
            Tok::Gt => BinOp::Gt,
            Tok::GtEq => BinOp::GtEq,
            _ => return None,
        })
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let l = self.parse_additive()?;
        if let Some(op) = Self::cmp_op(&self.peek()) {
            self.bump();
            self.skip_newlines();
            let r = self.parse_additive()?;
            let e = self.binary(op, l, r);
            if Self::cmp_op(&self.peek()).is_some() {
                let span = self.peek_span();
                return self.error_at(
                    "P010",
                    "comparison operators cannot be chained (use `and`)",
                    span,
                );
            }
            return Ok(e);
        }
        Ok(l)
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let mut l = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(l),
            };
            self.bump();
            self.skip_newlines();
            let r = self.parse_multiplicative()?;
            l = self.binary(op, l, r);
        }
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let mut l = self.parse_cast()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent | Tok::ModKw => BinOp::Mod,
                Tok::DivKw => BinOp::IntDiv,
                _ => return Ok(l),
            };
            self.bump();
            self.skip_newlines();
            let r = self.parse_cast()?;
            l = self.binary(op, l, r);
        }
    }

    fn parse_cast(&mut self) -> PResult<Expr> {
        let mut e = self.parse_unary()?;
        while self.at(&Tok::As) {
            self.bump();
            let ty = self.parse_type()?;
            let span = e.span.to(ty.span);
            e = self.mk(span, ExprKind::Cast(Box::new(e), ty));
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        if self.at(&Tok::Minus) {
            let start = self.bump().span;
            let e = self.parse_unary()?;
            let span = start.to(e.span);
            return Ok(self.mk(span, ExprKind::Unary(UnOp::Neg, Box::new(e))));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> PResult<Expr> {
        let base = self.parse_postfix()?;
        if self.at(&Tok::Power) {
            self.bump();
            self.skip_newlines();
            let exp = self.parse_unary()?;
            return Ok(self.binary(BinOp::Pow, base, exp));
        }
        Ok(base)
    }

    /// Collect `a`, `a.b`, `a.b.c` as a path if the expression is one.
    fn expr_path(e: &Expr) -> Option<(Vec<Ident>, Vec<TypeExpr>)> {
        match &e.kind {
            ExprKind::Ident(name, targs) => Some((
                vec![Ident {
                    name: name.clone(),
                    span: e.span,
                }],
                targs.clone(),
            )),
            ExprKind::Field(obj, name, targs) => {
                let (mut path, inner_targs) = Self::expr_path(obj)?;
                if !inner_targs.is_empty() {
                    return None;
                }
                path.push(name.clone());
                Some((path, targs.clone()))
            }
            _ => None,
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Tok::LParen => {
                    self.bump();
                    self.nl.push(false);
                    let r = self.parse_call_args();
                    self.nl.pop();
                    let (args, end) = r?;
                    let span = e.span.to(end);
                    e = self.mk(
                        span,
                        ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                    );
                }
                Tok::Dot if matches!(self.toks[self.pos + 1].tok, Tok::Int(_)) => {
                    // `t.0`: an element of a tuple
                    self.bump();
                    let Tok::Int(n) = self.peek() else {
                        unreachable!()
                    };
                    let end = self.bump().span;
                    let span = e.span.to(end);
                    e = self.mk(span, ExprKind::TupleField(Box::new(e), n as usize));
                }
                Tok::Dot => {
                    self.bump();
                    let name = self.expect_ident()?;
                    let span = e.span.to(name.span);
                    e = self.mk(span, ExprKind::Field(Box::new(e), name, Vec::new()));
                }
                Tok::Question => {
                    let end = self.bump().span;
                    let span = e.span.to(end);
                    e = self.mk(span, ExprKind::Try(Box::new(e)));
                }
                Tok::LBracket => {
                    self.bump();
                    self.nl.push(false);
                    if self.at(&Tok::RBracket) {
                        let end = self.bump().span;
                        self.nl.pop();
                        let span = e.span.to(end);
                        e = self.mk(span, ExprKind::Append(Box::new(e)));
                        continue;
                    }
                    let r = self
                        .parse_expr()
                        .and_then(|i| self.expect(&Tok::RBracket).map(|end| (i, end)));
                    self.nl.pop();
                    let (idx, end) = r?;
                    let span = e.span.to(end);
                    e = self.mk(span, ExprKind::Index(Box::new(e), Box::new(idx)));
                }
                Tok::Lt => {
                    // `name<int>(...)`, `Type<int>.new(...)`, `Type<int> { ... }`:
                    // type arguments only if `>` is followed by `(`, `.` or `{`.
                    let can_take = matches!(&e.kind, ExprKind::Ident(_, t) | ExprKind::Field(_, _, t) if t.is_empty());
                    if !can_take {
                        break;
                    }
                    let saved_pos = self.pos;
                    let saved_diags = self.diags.len();
                    let saved_nl = self.nl.len();
                    match self.parse_type_args() {
                        Ok(args) if matches!(self.peek(), Tok::LParen | Tok::Dot | Tok::LBrace) => {
                            match &mut e.kind {
                                ExprKind::Ident(_, t) | ExprKind::Field(_, _, t) => *t = args,
                                _ => unreachable!(),
                            }
                        }
                        _ => {
                            self.pos = saved_pos;
                            self.diags.truncate(saved_diags);
                            self.nl.truncate(saved_nl);
                            break;
                        }
                    }
                }
                Tok::LBrace => {
                    let Some((path, targs)) = Self::expr_path(&e) else {
                        break;
                    };
                    e = self.parse_struct_lit(e.span, path, targs)?;
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_call_args(&mut self) -> PResult<(Vec<CallArg>, Span)> {
        let mut args = Vec::new();
        while !self.at(&Tok::RParen) {
            // `name: value` is a named argument; anything else is positional.
            let name = if matches!(self.peek(), Tok::Ident(_)) && matches!(self.peek2(), Tok::Colon)
            {
                let name = self.expect_ident()?;
                self.bump();
                Some(name)
            } else {
                None
            };
            let value = self.parse_expr()?;
            args.push(CallArg { name, value });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let end = self.expect(&Tok::RParen)?;
        Ok((args, end))
    }

    fn parse_struct_lit(
        &mut self,
        start: Span,
        path: Vec<Ident>,
        targs: Vec<TypeExpr>,
    ) -> PResult<Expr> {
        self.expect(&Tok::LBrace)?;
        self.nl.push(false);
        let r: PResult<(Option<Box<Expr>>, Vec<StructLitItem>, Span)> = (|| {
            let mut base = None;
            let mut fields = Vec::new();
            while !self.at(&Tok::RBrace) {
                if self.at(&Tok::DotDot) {
                    let sp = self.bump().span;
                    if base.is_some() {
                        return self.error_at("P070", "only one `..base` is allowed", sp);
                    }
                    base = Some(Box::new(self.parse_expr()?));
                } else {
                    let name = self.expect_ident()?;
                    let value = if self.at(&Tok::Colon) {
                        self.bump();
                        self.parse_expr()?
                    } else if matches!(self.peek(), Tok::Comma | Tok::RBrace) {
                        // shorthand `Point { x, y }` means `Point { x: x, y: y }`
                        self.mk(name.span, ExprKind::Ident(name.name.clone(), Vec::new()))
                    } else {
                        let span = self.peek_span();
                        return self.error_at(
                            "P071",
                            format!("expected `:` and a value after field `{}`", name.name),
                            span,
                        );
                    };
                    fields.push(StructLitItem { name, value });
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            let end = self.expect(&Tok::RBrace)?;
            Ok((base, fields, end))
        })();
        self.nl.pop();
        let (base, fields, end) = r?;
        let span = start.to(end);
        Ok(self.mk(
            span,
            ExprKind::StructLit(Box::new(StructLit {
                path,
                targs,
                base,
                fields,
            })),
        ))
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let span = self.peek_span();
        match self.peek() {
            Tok::Int(v) => {
                self.bump();
                Ok(self.mk(span, ExprKind::Int(v)))
            }
            Tok::Float(v) => {
                self.bump();
                Ok(self.mk(span, ExprKind::Float(v)))
            }
            Tok::Str(raw) => {
                self.bump();
                let parts = self.parse_string(&raw, span);
                Ok(self.mk(span, ExprKind::Str(parts)))
            }
            Tok::True => {
                self.bump();
                Ok(self.mk(span, ExprKind::Bool(true)))
            }
            Tok::False => {
                self.bump();
                Ok(self.mk(span, ExprKind::Bool(false)))
            }
            Tok::None => {
                self.bump();
                Ok(self.mk(span, ExprKind::None))
            }
            Tok::Ident(name) => {
                self.bump();
                Ok(self.mk(span, ExprKind::Ident(name, Vec::new())))
            }
            Tok::SelfKw => {
                self.bump();
                Ok(self.mk(span, ExprKind::SelfVal))
            }
            Tok::Super => {
                self.bump();
                if !self.at(&Tok::Dot) {
                    return self.error_at(
                        "P080",
                        "`super` must be followed by a method call (`super.method(...)`)",
                        span,
                    );
                }
                Ok(self.mk(span, ExprKind::Super))
            }
            Tok::LParen => {
                self.bump();
                self.nl.push(false);
                // `(e)` groups; `(a, b)` is a tuple
                let r: PResult<Expr> = (|| {
                    let first = self.parse_expr()?;
                    if !self.at(&Tok::Comma) {
                        self.expect(&Tok::RParen)?;
                        return Ok(first);
                    }
                    let mut items = vec![first];
                    while self.eat(&Tok::Comma) {
                        if self.at(&Tok::RParen) {
                            break;
                        }
                        items.push(self.parse_expr()?);
                    }
                    let end = self.expect(&Tok::RParen)?;
                    if items.len() < 2 {
                        return self.error_at(
                            "P060",
                            "a tuple needs at least two values: `(1, \"a\")`",
                            span.to(end),
                        );
                    }
                    Ok(self.mk(span.to(end), ExprKind::Tuple(items)))
                })();
                self.nl.pop();
                r
            }
            Tok::LBracket => {
                self.bump();
                self.nl.push(false);
                let r: PResult<(Vec<Expr>, Span)> = (|| {
                    let mut items = Vec::new();
                    while !self.at(&Tok::RBracket) {
                        items.push(self.parse_expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(&Tok::RBracket)?;
                    Ok((items, end))
                })();
                self.nl.pop();
                let (items, end) = r?;
                Ok(self.mk(span.to(end), ExprKind::Array(items)))
            }
            Tok::LBrace => {
                self.bump();
                self.nl.push(false);
                let r: PResult<(Vec<(Expr, Expr)>, Span)> = (|| {
                    let mut items = Vec::new();
                    while !self.at(&Tok::RBrace) {
                        let k = self.parse_expr()?;
                        if !self.at(&Tok::Colon) {
                            return self.unexpected("`:` and a value after the key");
                        }
                        self.bump();
                        let v = self.parse_expr()?;
                        items.push((k, v));
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(&Tok::RBrace)?;
                    Ok((items, end))
                })();
                self.nl.pop();
                let (items, end) = r?;
                Ok(self.mk(span.to(end), ExprKind::Map(items)))
            }
            Tok::If => self.parse_if(),
            Tok::Match => self.parse_match(),
            Tok::Fun => {
                if matches!(self.toks[self.pos + 1].tok, Tok::Ident(_)) {
                    return self.error_at("P052", "named functions are only allowed at the top level; here write `let name = fun(...) ... end`", span);
                }
                let f = self.parse_fun(false, BodyMode::Required)?;
                let sp = f.span;
                Ok(self.mk(sp, ExprKind::Lambda(Rc::new(f))))
            }
            _ => self.unexpected("an expression"),
        }
    }

    fn parse_if(&mut self) -> PResult<Expr> {
        let start = self.expect(&Tok::If)?;
        let mut branches = Vec::new();
        let mut else_block = None;
        let cond = self.parse_expr()?;
        self.expect(&Tok::Then)?;
        let body = self.parse_block(&[Tok::ElseIf, Tok::Else, Tok::End]);
        branches.push((cond, body));
        loop {
            match self.peek() {
                Tok::ElseIf => {
                    self.bump();
                    let cond = self.parse_expr()?;
                    self.expect(&Tok::Then)?;
                    let body = self.parse_block(&[Tok::ElseIf, Tok::Else, Tok::End]);
                    branches.push((cond, body));
                }
                Tok::Else => {
                    self.bump();
                    else_block = Some(self.parse_block(&[Tok::End]));
                }
                _ => break,
            }
        }
        let end = self.expect(&Tok::End)?;
        Ok(self.mk(
            start.to(end),
            ExprKind::If {
                branches,
                else_block,
            },
        ))
    }

    fn parse_match(&mut self) -> PResult<Expr> {
        let start = self.expect(&Tok::Match)?;
        let scrutinee = self.parse_expr()?;
        let mut cases = Vec::new();
        let mut else_block = None;
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Case => {
                    let cstart = self.bump().span;
                    let pstart = self.peek_span();
                    let pattern = self.parse_pattern()?;
                    let pattern_span = pstart.to(self.prev_span());
                    let guard = if self.eat(&Tok::If) {
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };
                    self.expect(&Tok::Then)?;
                    let body = self.parse_block(&[Tok::Case, Tok::Else, Tok::End]);
                    cases.push(MatchCase {
                        span: cstart.to(self.prev_span()),
                        pattern,
                        pattern_span,
                        guard,
                        body,
                    });
                }
                Tok::Else => {
                    self.bump();
                    else_block = Some(self.parse_block(&[Tok::End]));
                    break;
                }
                _ => break,
            }
        }
        self.skip_newlines();
        let end = self.expect(&Tok::End)?;
        Ok(self.mk(
            start.to(end),
            ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                cases,
                else_block,
            },
        ))
    }

    fn parse_pattern_number(&mut self) -> PResult<Pattern> {
        let neg = self.eat(&Tok::Minus);
        match self.peek() {
            Tok::Int(v) => {
                self.bump();
                Ok(Pattern::Int(if neg { -v } else { v }))
            }
            Tok::Float(v) => {
                self.bump();
                Ok(Pattern::Float(if neg { -v } else { v }))
            }
            _ => self.unexpected("a number"),
        }
    }

    /// `(a, _, c)` after the path of a variant that carries data.
    fn parse_variant_pattern(&mut self, path: Vec<Ident>) -> PResult<Pattern> {
        self.expect(&Tok::LParen)?;
        self.nl.push(false);
        let mut fields = Vec::new();
        let r: PResult<()> = (|| {
            while !self.at(&Tok::RParen) {
                let name = self.expect_ident()?;
                fields.push(if name.name == "_" { None } else { Some(name) });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen)?;
            Ok(())
        })();
        self.nl.pop();
        r?;
        Ok(Pattern::Variant { path, fields })
    }

    fn parse_pattern(&mut self) -> PResult<Pattern> {
        match self.peek() {
            Tok::Minus | Tok::Int(_) | Tok::Float(_) => {
                let lo = self.parse_pattern_number()?;
                let inclusive = match self.peek() {
                    Tok::DotDot => false,
                    Tok::DotDotEq => true,
                    _ => return Ok(lo),
                };
                self.bump();
                let hi = self.parse_pattern_number()?;
                Ok(Pattern::Range {
                    lo: Box::new(lo),
                    hi: Box::new(hi),
                    inclusive,
                })
            }
            Tok::Str(raw) => {
                let span = self.bump().span;
                let parts = self.parse_string(&raw, span);
                let mut s = String::new();
                for p in parts {
                    match p {
                        StrPart::Lit(l) => s.push_str(&l),
                        StrPart::Expr(e) => {
                            return self.error_at(
                                "P090",
                                "a pattern cannot contain `{...}`",
                                e.span,
                            );
                        }
                    }
                }
                Ok(Pattern::Str(s))
            }
            Tok::True => {
                self.bump();
                Ok(Pattern::Bool(true))
            }
            Tok::False => {
                self.bump();
                Ok(Pattern::Bool(false))
            }
            Tok::None => {
                self.bump();
                Ok(Pattern::None)
            }
            Tok::LParen => {
                let start = self.bump().span;
                self.nl.push(false);
                let mut items = Vec::new();
                let r: PResult<Span> = (|| {
                    while !self.at(&Tok::RParen) {
                        items.push(self.parse_pattern()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(&Tok::RParen)
                })();
                self.nl.pop();
                let end = r?;
                if items.len() < 2 {
                    return self.error_at(
                        "P062",
                        "a tuple pattern needs at least two patterns: `(0, y)`",
                        start.to(end),
                    );
                }
                Ok(Pattern::Tuple(items))
            }
            Tok::Ident(_) => {
                let mut path = vec![self.expect_ident()?];
                while self.at(&Tok::Dot) {
                    self.bump();
                    path.push(self.expect_ident()?);
                }
                if self.at(&Tok::LParen) {
                    return self.parse_variant_pattern(path);
                }
                if path.len() == 1 {
                    Ok(Pattern::Bind(path.pop().unwrap()))
                } else {
                    Ok(Pattern::Path(path))
                }
            }
            _ => self.unexpected("a pattern (a value, a range, `Enum.Variant` or a name)"),
        }
    }

    // --------------------------------------------------------------- strings

    /// Split a string's raw content into literal text and `{expr}` parts.
    fn parse_string(&mut self, raw: &str, span: Span) -> Vec<StrPart> {
        let rb = raw.as_bytes();
        let mut parts = Vec::new();
        let mut cur = String::new();
        let mut i = 0;
        // offset of raw[0] in the file
        let base = span.start as usize + 1;
        while i < rb.len() {
            match rb[i] {
                b'\\' => {
                    let esc = rb.get(i + 1).copied();
                    match esc {
                        Some(b'n') => cur.push('\n'),
                        Some(b't') => cur.push('\t'),
                        Some(b'r') => cur.push('\r'),
                        Some(b'0') => cur.push('\0'),
                        Some(b'\\') => cur.push('\\'),
                        Some(b'"') => cur.push('"'),
                        Some(b'{') => cur.push('{'),
                        Some(b'}') => cur.push('}'),
                        _ => {
                            let width = raw
                                .get(i + 1..)
                                .and_then(|r| r.chars().next())
                                .map_or(0, char::len_utf8);
                            let s = Span::new(self.file, base + i, base + i + 1 + width);
                            crate::diag::push_capped(
                                &mut self.diags,
                                Diagnostic::error(
                                    "P020",
                                    "unknown escape sequence (allowed: \\n \\t \\r \\0 \\\\ \\\" \\{ \\})",
                                    s,
                                ),
                            );
                            // skip the whole (possibly multi-byte) character
                            i += 1 + width;
                            continue;
                        }
                    }
                    i += 2;
                }
                b'{' => {
                    let mut depth = 1;
                    let mut j = i + 1;
                    while j < rb.len() {
                        match rb[j] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            b'\\' => j += 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    if j >= rb.len() {
                        let s = Span::new(self.file, base + i, base + i + 1);
                        crate::diag::push_capped(
                            &mut self.diags,
                            Diagnostic::error(
                                "P021",
                                "unterminated `{` in string (write `\\{` for a literal brace)",
                                s,
                            ),
                        );
                        break;
                    }
                    let inner = &raw[i + 1..j];
                    let inner_base = base + i + 1;
                    if inner.trim().is_empty() {
                        let s = Span::new(self.file, base + i, base + j + 1);
                        crate::diag::push_capped(
                            &mut self.diags,
                            Diagnostic::error(
                                "P022",
                                "empty `{}` in string (write `\\{\\}` for literal braces)",
                                s,
                            ),
                        );
                    } else if let Some(e) = self.parse_interpolation(inner, inner_base) {
                        if !cur.is_empty() {
                            parts.push(StrPart::Lit(std::mem::take(&mut cur)));
                        }
                        parts.push(StrPart::Expr(e));
                    }
                    i = j + 1;
                }
                _ => {
                    let ch = raw[i..].chars().next().unwrap();
                    cur.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        if !cur.is_empty() || parts.is_empty() {
            parts.push(StrPart::Lit(cur));
        }
        parts
    }

    fn parse_interpolation(&mut self, inner: &str, base: usize) -> Option<Expr> {
        let toks = lex(inner, self.file, base, &mut self.diags);
        let mut sub = Parser::new(self.src, self.file, toks, self.next_id);
        sub.nl = vec![false];
        let e = sub.parse_expr();
        if e.is_ok() && !sub.at(&Tok::Eof) {
            let found = sub.peek();
            let span = sub.peek_span();
            crate::diag::push_capped(
                &mut sub.diags,
                Diagnostic::error(
                    "P023",
                    format!("unexpected {} in string interpolation", found.describe()),
                    span,
                ),
            );
        }
        self.next_id = sub.next_id;
        self.diags.append(&mut sub.diags);
        e.ok()
    }
}
