//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod body;
mod calls;
mod catalog;
mod info;
mod decls;
mod expr;
mod unify;

pub use info::{ExprInfo, Info, ItemInfo, LocalInfo, Member};

use crate::ast::{self, Item};
use crate::diag::{Diagnostic, MAX_DIAGNOSTICS, Severity};
use crate::host::{Capabilities, Host};
use crate::parser::parse_module_from;
use crate::span::{SourceMap, Span, resolve_module_path};
use crate::types::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct ModuleData {
    pub path: String,
    pub file: u32,
    pub ast: Rc<ast::Module>,
}

/// A checked program: sources, definitions and what each name refers to.
pub struct Program {
    pub sources: SourceMap,
    /// In execution order: a module comes after the modules it imports.
    pub modules: Vec<ModuleData>,
    pub defs: Defs,
    pub res: Resolutions,
    pub warnings: Vec<Diagnostic>,
    /// For editors; only when asked for (see [`CompileOptions`]).
    pub info: Option<Info>,
}

/// Compilation failed. Diagnostics carry resolved locations.
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    pub sources: SourceMap,
    /// What the checker learned before stopping (only when asked for).
    pub info: Option<Box<Info>>,
}

impl CompileError {
    pub fn render(&self) -> String {
        self.diagnostics.iter().map(|d| d.render(&self.sources)).collect::<Vec<_>>().join("\n")
    }
}

/// Compile the program whose entry file is `entry` with text `source`.
/// Imported modules are requested from `host`.
pub fn compile(entry: &str, source: &str, host: &mut dyn Host) -> Result<Program, CompileError> {
    compile_with(entry, source, host, &CompileOptions::default())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CompileOptions {
    /// Also collect the types of expressions, local variables and members (for editors).
    pub info: bool,
}

/// Like [`compile`], with options.
pub fn compile_with(entry: &str, source: &str, host: &mut dyn Host, options: &CompileOptions) -> Result<Program, CompileError> {
    // A bug in the compiler must not take the caller down with it (a server, an editor).
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compile_inner(entry, source, host, options))) {
        Ok(result) => result,
        Err(payload) => Err(CompileError {
            diagnostics: vec![internal_error(&payload)],
            sources: SourceMap::default(),
            info: None,
        }),
    }
}

/// A diagnostic for a panic caught inside qlang itself.
pub(crate) fn internal_error(payload: &Box<dyn std::any::Any + Send>) -> Diagnostic {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".to_string());
    Diagnostic::error("X999", format!("internal error: {detail} (this is a bug in qlang, please report it)"), Span::default())
}

/// The path given to the prelude's synthetic module, and the index it always loads at
/// (it is loaded before the entry file, and never imports anything itself).
const PRELUDE_PATH: &str = "<prelude>";
pub(crate) const PRELUDE_MOD: ModId = 0;

/// Declarations available in every file without an `import`: resolved like ordinary
/// qlang source (same checker, same diagnostics), just loaded as module 0 before the
/// entry file. A name declared locally always shadows one from here (`lookup_item`
/// checks the current module first).
const PRELUDE_SOURCE: &str = r#"
struct Error
    public message: string
end

struct Result<T, E>
    data: T?
    err: E?
end

impl<T, E> Result<T, E>
    --" An ok result holding `data`. --"
    public fun ok(data: T) -> Result<T, E>
        Result { data, err: none }
    end

    --" An error result holding `error`. --"
    public fun err(error: E) -> Result<T, E>
        Result { data: none, err: error }
    end

    --" Whether this is an ok result. --"
    public fun is_ok(self) -> bool
        self.data != none
    end

    --" Whether this is an error result. --"
    public fun is_err(self) -> bool
        self.err != none
    end

    --" The ok value, or stops the program if this is an error result. --"
    public fun unwrap(self) -> T
        let d = self.data
        if d != none then
            d
        else
            panic("called unwrap() on an error Result")
        end
    end

    --" The error value, or stops the program if this is an ok result. --"
    public fun unwrap_err(self) -> E
        let e = self.err
        if e != none then
            e
        else
            panic("called unwrap_err() on an ok Result")
        end
    end
end

--" An ok `Result`, usually inferred from context (`return Ok(1)` inside a function
returning `Result<int, Error>`, `let r: Result<int, Error> = Ok(1)`...). When the
compiler cannot infer `E` from anywhere, write `Result<int, Error>.ok(1)` directly. --"
fun Ok<T, E>(value: T) -> Result<T, E>
    Result<T, E>.ok(value)
end

--" An error `Result`: see `Ok`. --"
fun Err<T, E>(error: E) -> Result<T, E>
    Result<T, E>.err(error)
end

-- ===== encode =====

--" A type that can be turned into JSON text: `impl ToJson for Post ... end`. --"
trait ToJson
    fun to_json(self) -> string
end

--" A JSON string literal of `s`, quoted and escaped. --"
fun json_escape(s: string) -> string
    let out = "\""
    for c in s.chars() do
        if c == "\"" then
            out += "\\\""
        elseif c == "\\" then
            out += "\\\\"
        elseif c == "\n" then
            out += "\\n"
        elseif c == "\t" then
            out += "\\t"
        elseif c == "\r" then
            out += "\\r"
        else
            out += c
        end
    end
    out + "\""
end

--" A JSON array of `items`, each serialized through `to_json()`. --"
fun json_array<T: ToJson>(items: array<T>) -> string
    let parts: array<string> = []
    for it in items do
        parts[] = it.to_json()
    end
    "[" + parts.join(",") + "]"
end

--" A JSON object of `fields`, each value serialized through `to_json()`. Used from a
struct's own `to_json`: `json_object({ "title": self.title, "views": self.views })`. --"
fun json_object(fields: map<string, ToJson>) -> string
    let parts: array<string> = []
    for k, v in fields do
        parts[] = json_escape(k) + ":" + v.to_json()
    end
    "\{" + parts.join(",") + "\}"
end

--" Merges two JSON objects (as produced by `json_object`) into one with the keys of
both. Lets a struct's `to_json` extend its parent's without repeating its fields:
`json_merge(super.to_json(), json_object({ "role": self.role }))`. --"
fun json_merge(a: string, b: string) -> string
    if a == "\{\}" then
        return b
    end
    if b == "\{\}" then
        return a
    end
    a.substring(0, a.len() - 1) + "," + b.substring(1, b.len())
end

impl ToJson for int
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for float
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for bool
    public fun to_json(self) -> string
        self as string
    end
end

impl ToJson for string
    public fun to_json(self) -> string
        json_escape(self)
    end
end

impl ToJson for uuid
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

impl ToJson for ulid
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

impl ToJson for datetime
    public fun to_json(self) -> string
        json_escape(self as string)
    end
end

--" So an `array<T>` of any `ToJson` type (including a nested struct or another
array) can be used directly as a `json_object` field, with no need to call
`json_array` by hand. --"
impl<T: ToJson> ToJson for array<T>
    public fun to_json(self) -> string
        json_array(self)
    end
end

-- ===== value tree (decode target) =====

--" A parsed JSON value: a string, a number, a bool, null, an array or an object.
Build one with `Json.string(...)`, `Json.number(...)`, `Json.array(...)`... (or parse
text with `Json.parse`), read it back with `is_string()`/`as_string()` and friends. --"
struct Json
    kind: string
    str: string?
    num: float?
    bool_: bool?
    arr: array<Json>?
    obj: map<string, Json>?
end

impl Json
    public fun null() -> Json
        Json { kind: "null", str: none, num: none, bool_: none, arr: none, obj: none }
    end

    public fun string(value: string) -> Json
        Json { kind: "string", str: value, num: none, bool_: none, arr: none, obj: none }
    end

    public fun number(value: float) -> Json
        Json { kind: "number", str: none, num: value, bool_: none, arr: none, obj: none }
    end

    public fun boolean(value: bool) -> Json
        Json { kind: "bool", str: none, num: none, bool_: value, arr: none, obj: none }
    end

    public fun array(value: array<Json>) -> Json
        Json { kind: "array", str: none, num: none, bool_: none, arr: value, obj: none }
    end

    public fun object(value: map<string, Json>) -> Json
        Json { kind: "object", str: none, num: none, bool_: none, arr: none, obj: value }
    end

    public fun is_null(self) -> bool
        self.kind == "null"
    end

    public fun is_string(self) -> bool
        self.kind == "string"
    end

    public fun is_number(self) -> bool
        self.kind == "number"
    end

    public fun is_bool(self) -> bool
        self.kind == "bool"
    end

    public fun is_array(self) -> bool
        self.kind == "array"
    end

    public fun is_object(self) -> bool
        self.kind == "object"
    end

    public fun as_string(self) -> string
        let v = self.str
        if v != none then v else panic("Json: not a string") end
    end

    public fun as_number(self) -> float
        let v = self.num
        if v != none then v else panic("Json: not a number") end
    end

    public fun as_bool(self) -> bool
        let v = self.bool_
        if v != none then v else panic("Json: not a bool") end
    end

    public fun as_array(self) -> array<Json>
        let v = self.arr
        if v != none then v else panic("Json: not an array") end
    end

    public fun as_object(self) -> map<string, Json>
        let v = self.obj
        if v != none then v else panic("Json: not an object") end
    end

    public fun get(self, key: string) -> Json?
        let o = self.obj
        if o != none then o[key] else none end
    end

    public fun at(self, i: int) -> Json?
        let a = self.arr
        if a != none and i >= 0 and i < a.len() then a[i] else none end
    end
end

impl ToJson for Json
    public fun to_json(self) -> string
        if self.kind == "null" then
            "null"
        elseif self.kind == "string" then
            json_escape(self.as_string())
        elseif self.kind == "number" then
            self.as_number() as string
        elseif self.kind == "bool" then
            self.as_bool() as string
        elseif self.kind == "array" then
            let out: array<ToJson> = []
            for it in self.as_array() do
                out[] = it
            end
            json_array(out)
        else
            let fields: map<string, ToJson> = {}
            for k, v in self.as_object() do
                fields[k] = v
            end
            json_object(fields)
        end
    end
end

-- ===== parse =====

fun hex_digit_value(c: string) -> int?
    if c.is_digit() then
        c.to_int()
    elseif c == "a" or c == "A" then
        10
    elseif c == "b" or c == "B" then
        11
    elseif c == "c" or c == "C" then
        12
    elseif c == "d" or c == "D" then
        13
    elseif c == "e" or c == "E" then
        14
    elseif c == "f" or c == "F" then
        15
    else
        none
    end
end

fun parse_hex4(hex: string) -> int?
    if hex.len() != 4 then
        return none
    end
    let a = hex_digit_value(hex.substring(0, 1))
    let b = hex_digit_value(hex.substring(1, 2))
    let c = hex_digit_value(hex.substring(2, 3))
    let d = hex_digit_value(hex.substring(3, 4))
    if a == none or b == none or c == none or d == none then
        return none
    end
    a * 4096 + b * 256 + c * 16 + d
end

struct JsonParser
    text: string
    pos: int
end

impl JsonParser
    public fun new(text: string) -> JsonParser
        JsonParser { text, pos: 0 }
    end

    public fun peek(self) -> string
        if self.pos < self.text.len() then self.text.substring(self.pos, self.pos + 1) else "" end
    end

    public fun advance(self)
        self.pos = self.pos + 1
    end

    public fun skip_ws(self)
        while self.peek() == " " or self.peek() == "\t" or self.peek() == "\n" or self.peek() == "\r" do
            self.advance()
        end
    end

    public fun looking_at(self, word: string) -> bool
        let end_ = (self.pos + word.len()).min(self.text.len())
        self.text.substring(self.pos, end_) == word
    end

    public fun fail(self, message: string) -> Result<Json, Error>
        Result<Json, Error>.err(Error { message: "JSON parse error at offset {self.pos}: {message}" })
    end

    public fun trailing_error(self) -> Error?
        self.skip_ws()
        if self.pos != self.text.len() then
            Error { message: "JSON parse error at offset {self.pos}: unexpected trailing characters" }
        else
            none
        end
    end

    public fun parse_value(self) -> Result<Json, Error>
        self.skip_ws()
        let c = self.peek()
        if c == "\"" then
            self.parse_string_value()
        elseif c == "\{" then
            self.parse_object()
        elseif c == "[" then
            self.parse_array()
        elseif self.looking_at("true") then
            self.pos = self.pos + 4
            Result<Json, Error>.ok(Json.boolean(true))
        elseif self.looking_at("false") then
            self.pos = self.pos + 5
            Result<Json, Error>.ok(Json.boolean(false))
        elseif self.looking_at("null") then
            self.pos = self.pos + 4
            Result<Json, Error>.ok(Json.null())
        elseif c == "-" or c.is_digit() then
            self.parse_number()
        else
            self.fail("unexpected character {json_escape(c)}")
        end
    end

    public fun parse_raw_string(self) -> Result<string, Error>
        self.advance()
        let out = ""
        while self.peek() != "\"" do
            if self.peek() == "" then
                return Result<string, Error>.err(Error { message: "JSON parse error at offset {self.pos}: unterminated string" })
            end
            if self.peek() == "\\" then
                self.advance()
                let e = self.peek()
                if e == "\"" then
                    out += "\""
                elseif e == "\\" then
                    out += "\\"
                elseif e == "/" then
                    out += "/"
                elseif e == "n" then
                    out += "\n"
                elseif e == "t" then
                    out += "\t"
                elseif e == "r" then
                    out += "\r"
                elseif e == "b" then
                    out += (8).char()
                elseif e == "f" then
                    out += (12).char()
                elseif e == "u" then
                    let hex = self.text.substring(self.pos + 1, self.pos + 5)
                    let code = parse_hex4(hex)
                    if code == none then
                        return Result<string, Error>.err(Error { message: "JSON parse error at offset {self.pos}: invalid \\u escape" })
                    end
                    out += code.char()
                    self.pos = self.pos + 4
                else
                    return Result<string, Error>.err(Error { message: "JSON parse error at offset {self.pos}: invalid escape \\{e}" })
                end
                self.advance()
            else
                out += self.peek()
                self.advance()
            end
        end
        self.advance()
        Result<string, Error>.ok(out)
    end

    public fun parse_string_value(self) -> Result<Json, Error>
        let r = self.parse_raw_string()
        if r.is_err() then
            return Result<Json, Error>.err(r.unwrap_err())
        end
        Result<Json, Error>.ok(Json.string(r.unwrap()))
    end

    public fun parse_number(self) -> Result<Json, Error>
        let start = self.pos
        if self.peek() == "-" then
            self.advance()
        end
        while self.peek().is_digit() do
            self.advance()
        end
        if self.peek() == "." then
            self.advance()
            while self.peek().is_digit() do
                self.advance()
            end
        end
        if self.peek() == "e" or self.peek() == "E" then
            self.advance()
            if self.peek() == "+" or self.peek() == "-" then
                self.advance()
            end
            while self.peek().is_digit() do
                self.advance()
            end
        end
        let text = self.text.substring(start, self.pos)
        let n = text.to_float()
        if n == none then
            return Result<Json, Error>.err(Error { message: "JSON parse error at offset {start}: invalid number" })
        end
        Result<Json, Error>.ok(Json.number(n))
    end

    public fun parse_array(self) -> Result<Json, Error>
        self.advance()
        let items: array<Json> = []
        self.skip_ws()
        if self.peek() == "]" then
            self.advance()
            return Result<Json, Error>.ok(Json.array(items))
        end
        while true do
            let r = self.parse_value()
            if r.is_err() then
                return Result<Json, Error>.err(r.unwrap_err())
            end
            items[] = r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "]" then
                self.advance()
                return Result<Json, Error>.ok(Json.array(items))
            else
                return self.fail("expected ',' or ']'")
            end
        end
        self.fail("unreachable")
    end

    public fun parse_object(self) -> Result<Json, Error>
        self.advance()
        let fields: map<string, Json> = {}
        self.skip_ws()
        if self.peek() == "\}" then
            self.advance()
            return Result<Json, Error>.ok(Json.object(fields))
        end
        while true do
            self.skip_ws()
            if self.peek() != "\"" then
                return self.fail("expected a string key")
            end
            let key_r = self.parse_raw_string()
            if key_r.is_err() then
                return Result<Json, Error>.err(key_r.unwrap_err())
            end
            self.skip_ws()
            if self.peek() != ":" then
                return self.fail("expected ':'")
            end
            self.advance()
            let value_r = self.parse_value()
            if value_r.is_err() then
                return Result<Json, Error>.err(value_r.unwrap_err())
            end
            fields[key_r.unwrap()] = value_r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "\}" then
                self.advance()
                return Result<Json, Error>.ok(Json.object(fields))
            else
                return self.fail("expected ',' or '\}'")
            end
        end
        self.fail("unreachable")
    end
end

impl Json
    --" Parses JSON text into a `Json` value, or an `Error` describing what went wrong. --"
    public fun parse(text: string) -> Result<Json, Error>
        let p = JsonParser.new(text)
        let r = p.parse_value()
        if r.is_err() then
            return r
        end
        let trailing = p.trailing_error()
        if trailing != none then
            return Result<Json, Error>.err(trailing)
        end
        r
    end
end

--" An HTTP request, given to `http.send`. --"
struct Request
    public method: string
    public url: string
    public headers: map<string, string> = {}
    public body: string? = none
end

--" An HTTP response, returned by `http.send`. --"
struct Response
    public status: int
    public headers: map<string, string> = {}
    public body: string
end

--" A SQL database connection, opened with `Db.connect(url)`. `query`, `execute` and
`close` are native: this struct declares no method in qlang. --"
struct Db
    conn: int
end
"#;

fn compile_inner(entry: &str, source: &str, host: &mut dyn Host, options: &CompileOptions) -> Result<Program, CompileError> {
    let caps = host.capabilities();
    let mut loader = Loader {
        host,
        sources: SourceMap::default(),
        diags: Vec::new(),
        modules: Vec::new(),
        done: HashMap::new(),
        loading: Vec::new(),
        next_id: 0,
    };
    loader.load(PRELUDE_PATH, PRELUDE_PATH.to_string(), PRELUDE_SOURCE.to_string());
    let entry_key = loader.host.canonical_path(entry);
    loader.load(entry, entry_key, source.to_string());
    let Loader { sources, mut diags, modules, .. } = loader;

    let has_errors = |d: &[Diagnostic]| d.iter().any(|x| x.severity == Severity::Error);
    if has_errors(&diags) {
        return Err(finish_error(diags, sources, None));
    }

    let mut ck = Checker::new(sources, modules, caps);
    if options.info {
        ck.collect = Some(info::Collector::default());
    }
    ck.run();
    diags.append(&mut ck.diags);
    let info = if options.info { ck.build_info() } else { None };
    if has_errors(&diags) {
        return Err(finish_error(diags, ck.sources, info));
    }
    for d in &mut diags {
        d.resolve(&ck.sources);
    }
    Ok(Program {
        sources: ck.sources,
        modules: ck
            .modules
            .into_iter()
            .map(|m| ModuleData { path: m.path, file: m.file, ast: m.ast })
            .collect(),
        defs: ck.defs,
        res: ck.res,
        warnings: diags,
        info,
    })
}

fn finish_error(mut diags: Vec<Diagnostic>, sources: SourceMap, info: Option<Info>) -> CompileError {
    // stable order: by file then position
    diags.sort_by_key(|d| (d.span.file, d.span.start));
    if diags.len() > MAX_DIAGNOSTICS {
        let last = diags[MAX_DIAGNOSTICS - 1].span;
        diags.truncate(MAX_DIAGNOSTICS);
        diags.push(Diagnostic::error(
            "T000",
            format!("too many errors: only the first {MAX_DIAGNOSTICS} are shown"),
            last,
        ));
    }
    for d in &mut diags {
        d.resolve(&sources);
    }
    CompileError { diagnostics: diags, sources, info: info.map(Box::new) }
}

// ------------------------------------------------------------------ loading

pub(crate) struct Loaded {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    /// Item index of each `import` to the index of the module it loads.
    imports: HashMap<usize, usize>,
}

struct Loader<'h> {
    host: &'h mut dyn Host,
    sources: SourceMap,
    diags: Vec<Diagnostic>,
    modules: Vec<Loaded>,
    done: HashMap<String, usize>,
    loading: Vec<String>,
    /// Node ids are unique across all modules.
    next_id: u32,
}

impl Loader<'_> {
    /// `key` identifies the module (see `Host::canonical_path`); `path` is how it is named.
    fn load(&mut self, path: &str, key: String, text: String) -> usize {
        let file = self.sources.add(path, &text);
        self.loading.push(key.clone());
        let (ast, next_id) = parse_module_from(&text, file, self.next_id, &mut self.diags);
        self.next_id = next_id;
        let mut imports = HashMap::new();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(target) = resolve_module_path(path, &imp.path) else {
                crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T100",
                    format!(
                        "cannot import \"{}\": use a path relative to this file, written with `/` (`./x.q`, `../lib/x.q`), not an absolute one",
                        imp.path
                    ),
                    imp.path_span,
                ));
                continue;
            };
            let target_key = self.host.canonical_path(&target);
            if self.loading.contains(&target_key) {
                crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T101",
                    format!("circular import of \"{}\"", imp.path),
                    imp.path_span,
                ));
                continue;
            }
            if let Some(&m) = self.done.get(&target_key) {
                imports.insert(idx, m);
                continue;
            }
            match self.host.load_module(&target) {
                Ok(src) => {
                    let m = self.load(&target, target_key, src);
                    imports.insert(idx, m);
                }
                Err(e) => crate::diag::push_capped(&mut self.diags, Diagnostic::error(
                    "T102",
                    format!("cannot import \"{}\": {e}", imp.path),
                    imp.path_span,
                )),
            }
        }
        self.loading.pop();
        let idx = self.modules.len();
        self.modules.push(Loaded { path: path.to_string(), file, ast: Rc::new(ast), imports });
        self.done.insert(key, idx);
        idx
    }
}

// ------------------------------------------------------------------ checker

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ItemRef {
    Fn(FnId),
    Struct(StructId),
    Enum(EnumId),
    Trait(TraitId),
    Var(ModId, String),
    Module(ModId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalKind {
    Var,
    Const,
    Param,
}

#[derive(Clone, Debug)]
pub(crate) struct Local {
    pub ty: Ty,
    pub kind: LocalKind,
    /// Set when `ty` is a narrowed view (`x != none`) of this declared type.
    pub orig: Option<Ty>,
    pub span: Span,
}

#[derive(Default)]
pub(crate) struct ModScope {
    pub items: HashMap<String, ItemRef>,
    pub exports: HashSet<String>,
    /// Top-level variables, by name.
    pub vars: HashMap<String, Local>,
    /// Item index in the file to the id of its definition.
    pub decl_ids: HashMap<usize, usize>,
}

pub(crate) struct Checker {
    pub caps: Capabilities,
    pub sources: SourceMap,
    pub diags: Vec<Diagnostic>,
    pub defs: Defs,
    pub res: Resolutions,
    pub modules: Vec<Loaded>,
    pub mscopes: Vec<ModScope>,

    // --- state of the body being checked
    pub cur_mod: ModId,
    pub scopes: Vec<HashMap<String, Local>>,
    pub tparam_scope: Vec<(String, ParamId)>,
    /// Return type of the enclosing function or lambda.
    pub ret_stack: Vec<Ty>,
    pub loop_depth: usize,
    /// The struct whose `impl` is being checked (for visibility).
    pub cur_impl_struct: Option<StructId>,
    /// The type of `self` in the current method.
    pub cur_self: Option<Ty>,
    /// The method being checked (for `super`).
    pub cur_fn: Option<FnId>,
    pub subst: Vec<Option<Ty>>,
    /// Set when an editor asked for [`Info`].
    pub collect: Option<info::Collector>,
    /// The documentation comments (filled when building [`Info`]).
    pub(crate) docs: info::Docs,
}

impl Checker {
    fn new(sources: SourceMap, modules: Vec<Loaded>, caps: Capabilities) -> Checker {
        let n = modules.len();
        Checker {
            caps,
            sources,
            diags: Vec::new(),
            defs: Defs::new(),
            res: Resolutions::default(),
            modules,
            mscopes: (0..n).map(|_| ModScope::default()).collect(),
            cur_mod: 0,
            scopes: Vec::new(),
            tparam_scope: Vec::new(),
            ret_stack: Vec::new(),
            loop_depth: 0,
            cur_impl_struct: None,
            cur_self: None,
            cur_fn: None,
            subst: Vec::new(),
            collect: None,
            docs: info::Docs::default(),
        }
    }

    pub fn err(&mut self, code: &str, msg: impl Into<String>, span: Span) {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span));
    }

    pub fn err_note(&mut self, code: &str, msg: impl Into<String>, span: Span, note: impl Into<String>, note_span: Option<Span>) {
        crate::diag::push_capped(&mut self.diags, Diagnostic::error(code, msg, span).with_note(note, note_span));
    }

    pub fn show(&self, t: &Ty) -> String {
        let t = self.resolve(t);
        t.show(&self.defs)
    }

    fn run(&mut self) {
        let n = self.modules.len();
        // A: declare names
        for m in 0..n {
            self.cur_mod = m;
            self.declare_module(m);
        }
        // B: resolve signatures
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_traits(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_structs(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_functions(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_impls(m);
        }
        self.validate_structs();
        self.validate_overrides();
        if self.diags.iter().any(|d| d.severity == Severity::Error) {
            return;
        }
        // C: bodies
        for m in 0..n {
            self.cur_mod = m;
            self.check_module_body(m);
        }
    }
}
