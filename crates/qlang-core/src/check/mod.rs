//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod body;
mod calls;
mod captured;
mod catalog;
mod decls;
mod expr;
mod info;
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
        self.diagnostics
            .iter()
            .map(|d| d.render(&self.sources))
            .collect::<Vec<_>>()
            .join("\n")
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
pub fn compile_with(
    entry: &str,
    source: &str,
    host: &mut dyn Host,
    options: &CompileOptions,
) -> Result<Program, CompileError> {
    // A bug in the compiler must not take the caller down with it (a server, an editor).
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_inner(entry, source, host, options)
    })) {
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
    Diagnostic::error(
        "X999",
        format!("internal error: {detail} (this is a bug in qlang, please report it)"),
        Span::default(),
    )
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

--" An HTTP request: the one given to `http.send`, and the one a program run with
`qlang serve` receives. For a received request, `url` is the target as received
(`/items?id=3`), `path` its decoded path (`/items`) and `query` the decoded parameters
(`id` is `"3"`; the last one wins when a name is repeated); `headers` has lower-case names.
`params` holds the parameters of the route a `Router` matched (`/items/:id`). --"
struct Request
    public method: string
    public url: string
    public headers: map<string, string> = {}
    public body: string? = none
    public path: string = ""
    public query: map<string, string> = {}
    public params: map<string, string> = {}
end

impl Request
    --" The value of the header `name` of a received request (whatever the case of `name`),
    or `none`. --"
    public fun header(self, name: string) -> string?
        self.headers[name.lower()]
    end
end

--" An HTTP response: the one returned by `http.send`, and the one the `handle` function of
a program run with `qlang serve` returns. --"
struct Response
    public status: int
    public headers: map<string, string> = {}
    public body: string
end

impl Response
    --" A plain text response (status 200). --"
    public fun text(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "text/plain; charset=utf-8" }, body }
    end

    --" An HTML response (status 200). --"
    public fun html(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "text/html; charset=utf-8" }, body }
    end

    --" A JSON response (status 200) with the JSON text `body`. --"
    public fun json(body: string) -> Response
        Response { status: 200, headers: { "Content-Type": "application/json; charset=utf-8" }, body }
    end

    --" A redirection (status 302) to `location`. --"
    public fun redirect(location: string) -> Response
        Response { status: 302, headers: { "Location": location }, body: "" }
    end

    --" The 404 response. --"
    public fun not_found() -> Response
        Response { status: 404, headers: { "Content-Type": "text/plain; charset=utf-8" }, body: "Not Found" }
    end

    --" This response with another status (returns the response itself). --"
    public fun with_status(self, code: int) -> Response
        self.status = code
        self
    end

    --" This response with one more header (returns the response itself). --"
    public fun with_header(self, name: string, value: string) -> Response
        self.headers[name] = value
        self
    end
end

--" One route of a `Router`: the requests with this `method` (`"GET"`...) and a path matching
`pattern` go to `handler`. --"
struct Route
    public method: string
    public pattern: string
    public handler: fun(Request) -> Response
end

--" Sends each request to the route that matches it, so that a `handle` function does not need a
`match` on the path:

```
let router = Router.new()
router.get("/", fun(req: Request) -> Response Response.text("home") end)
router.get("/items/:id", fun(req: Request) -> Response
    Response.text("item " + req.params.get("id", ""))
end)

fun handle(req: Request) -> Response
    router.handle(req)
end
```

A pattern is made of segments between `/`: a word must match exactly, `:name` matches any
non-empty segment and puts it in `req.params`, and a final `*` matches the rest of the path
(`req.params["*"]`). The first route that matches wins. A path that matches a route with
another method gets a 405 response (with an `Allow` header), any other path a 404 --"
struct Router
    routes: array<Route>
    other: (fun(Request) -> Response)?
end

impl Router
    --" A router with no route. --"
    public fun new() -> Router
        Router { routes: [], other: none }
    end

    --" Adds a route for the requests with this `method` (`"GET"`, `"POST"`...). Returns the
    router, so calls can be chained. --"
    public fun add(self, method: string, pattern: string, handler: fun(Request) -> Response) -> Router
        self.routes[] = Route { method, pattern, handler }
        self
    end

    --" Adds a `GET` route. --"
    public fun get(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("GET", pattern, handler)
    end

    --" Adds a `POST` route. --"
    public fun post(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("POST", pattern, handler)
    end

    --" Adds a `PUT` route. --"
    public fun put(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("PUT", pattern, handler)
    end

    --" Adds a `DELETE` route. --"
    public fun delete(self, pattern: string, handler: fun(Request) -> Response) -> Router
        self.add("DELETE", pattern, handler)
    end

    --" What to answer when no route matches (instead of the 404 response). --"
    public fun fallback(self, handler: fun(Request) -> Response) -> Router
        self.other = handler
        self
    end

    --" The parameters of `path` if it matches `pattern`, or `none`. `/items/:id` matches
    `/items/3` (`id` is `"3"`); a final `*` takes the rest. --"
    public fun matches(pattern: string, path: string) -> map<string, string>?
        let wanted = pattern.split("/")
        let parts = path.split("/")
        let params: map<string, string> = {}
        let i = 0
        while i < wanted.len() do
            let w = wanted[i]
            if w == "*" and i == wanted.len() - 1 then
                if parts.len() <= i then
                    return none
                end
                params["*"] = parts.slice(i, parts.len()).join("/")
                return params
            end
            if i >= parts.len() then
                return none
            end
            if w.starts_with(":") then
                if parts[i] == "" then
                    return none
                end
                params[w.substring(1, w.len())] = parts[i]
            elseif w != parts[i] then
                return none
            end
            i += 1
        end
        if parts.len() != wanted.len() then none else params end
    end

    --" Answers `req` with the first route that matches it. --"
    public fun handle(self, req: Request) -> Response
        let allowed: array<string> = []
        for route in self.routes do
            let found = Router.matches(route.pattern, req.path)
            if found != none then
                if route.method == req.method then
                    req.params = found
                    return route.handler(req)
                end
                if not allowed.contains(route.method) then
                    allowed[] = route.method
                end
            end
        end
        let other = self.other
        if other != none then
            return other(req)
        end
        if allowed.len() > 0 then
            return Response { status: 405, headers: { "Allow": allowed.join(", "), "Content-Type": "text/plain; charset=utf-8" }, body: "Method Not Allowed" }
        end
        Response.not_found()
    end
end

-- The host provides the database: these are the `extern fun` it implements.
extern fun sql_connect(url: string) -> Result<int, Error>
extern fun sql_query(conn: int, sql: string, params: array<Json>) -> Result<array<map<string, Json>>, Error>
extern fun sql_execute(conn: int, sql: string, params: array<Json>) -> Result<int, Error>
extern fun sql_close(conn: int) -> Result<bool, Error>

--" Statement outcome as a `bool`: ok(true), or the same error. --"
fun sql_done(r: Result<int, Error>) -> Result<bool, Error>
    if r.is_ok() then
        Result.ok(true)
    else
        Result.err(r.unwrap_err())
    end
end

-- The host provides the disk, the process environment and the network too.
extern fun fs_read(path: string) -> Result<string, Error>
extern fun fs_write(path: string, content: string) -> Result<bool, Error>
extern fun env_get(name: string) -> string?
extern fun env_args() -> array<string>
extern fun http_send(req: Request) -> Result<Response, Error>

--" Files on disk: `fs.read(path)` and `fs.write(path, content)`. Needs disk access. --"
struct fs
end

impl fs
    --" Reads a file's whole content as text. Needs disk access. A missing file or a permission error is `err`, not a crash. --"
    public fun read(path: string) -> Result<string, Error>
        fs_read(path)
    end

    --" Writes `content` to a file, replacing it entirely (creating it if needed). Needs disk access. --"
    public fun write(path: string, content: string) -> Result<bool, Error>
        fs_write(path, content)
    end
end

--" The process environment: `env.get(name)` and `env.args()`. Needs a process environment. --"
struct env
end

impl env
    --" The value of the environment variable `name`, or `none` if it is not set. Needs a process environment. --"
    public fun get(name: string) -> string?
        env_get(name)
    end

    --" The program's own command-line arguments. Needs a process environment. --"
    public fun args() -> array<string>
        env_args()
    end
end

--" The network: `http.send(request)`. Needs network access. --"
struct http
end

impl http
    --" Sends an HTTP request and waits for the response. Needs network access. A connection failure is `err`, not a crash. --"
    public fun send(req: Request) -> Result<Response, Error>
        http_send(req)
    end
end

--" Stops the program (a failed test) unless `actual == expected`, showing both values. --"
fun assert_eq<T: Eq>(actual: T, expected: T)
    if actual != expected then
        panic("assert_eq failed\n  actual:   {actual}\n  expected: {expected}")
    end
end

--" Stops the program (a failed test) if `actual == unexpected`, showing the value. --"
fun assert_ne<T: Eq>(actual: T, unexpected: T)
    if actual == unexpected then
        panic("assert_ne failed: both sides are {actual}")
    end
end

--" A SQL database connection, opened with `Db.connect(url)`. --"
struct Db
    conn: int
end

impl Db
    --" Opens a database connection (`sqlite://path`, `postgres://...`). Needs database access. --"
    public fun connect(url: string) -> Result<Db, Error>
        let r = sql_connect(url)
        if r.is_ok() then
            Result.ok(Db { conn: r.unwrap() })
        else
            Result.err(r.unwrap_err())
        end
    end

    --" Runs a statement that returns rows (`SELECT`). `?` placeholders are bound from `params`, in order. --"
    public fun query(self, sql: string, params: array<Json>) -> Result<array<map<string, Json>>, Error>
        sql_query(self.conn, sql, params)
    end

    --" Runs a statement that does not return rows (`INSERT`/`UPDATE`/`DELETE`/DDL), returning the number of rows affected. --"
    public fun execute(self, sql: string, params: array<Json>) -> Result<int, Error>
        sql_execute(self.conn, sql, params)
    end

    --" Closes the connection. --"
    public fun close(self) -> Result<bool, Error>
        sql_close(self.conn)
    end

    --" Starts a transaction. --"
    public fun begin(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "BEGIN", []))
    end

    --" Commits the transaction started by `begin()`. --"
    public fun commit(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "COMMIT", []))
    end

    --" Cancels everything done since `begin()`. --"
    public fun rollback(self) -> Result<bool, Error>
        sql_done(sql_execute(self.conn, "ROLLBACK", []))
    end
end
"#;

fn compile_inner(
    entry: &str,
    source: &str,
    host: &mut dyn Host,
    options: &CompileOptions,
) -> Result<Program, CompileError> {
    let caps = host.capabilities();
    let mut loader = Loader {
        host,
        sources: SourceMap::default(),
        diags: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        done: HashMap::new(),
        next_id: 0,
    };
    loader.discover(
        PRELUDE_PATH,
        PRELUDE_PATH.to_string(),
        PRELUDE_SOURCE.to_string(),
    );
    let entry_key = loader.host.canonical_path(entry);
    loader.discover(entry, entry_key, source.to_string());
    let (sources, mut diags, modules) = loader.finish();

    let has_errors = |d: &[Diagnostic]| d.iter().any(|x| x.severity == Severity::Error);
    if has_errors(&diags) {
        return Err(finish_error(diags, sources, None));
    }

    let missing_externs: HashSet<String> = modules
        .iter()
        .flat_map(|m| m.ast.items.iter())
        .filter_map(|item| match item {
            Item::Fun(f) if f.external => f.name.as_ref().map(|n| n.name.clone()),
            _ => None,
        })
        .filter(|name| !host.has_extern(name))
        .collect();
    let mut ck = Checker::new(sources, modules, caps);
    ck.missing_externs = missing_externs;
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
            .map(|m| ModuleData {
                path: m.path,
                file: m.file,
                ast: m.ast,
            })
            .collect(),
        defs: ck.defs,
        res: ck.res,
        warnings: diags,
        info,
    })
}

fn finish_error(
    mut diags: Vec<Diagnostic>,
    sources: SourceMap,
    info: Option<Info>,
) -> CompileError {
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
    CompileError {
        diagnostics: diags,
        sources,
        info: info.map(Box::new),
    }
}

// ------------------------------------------------------------------ loading

pub(crate) struct Loaded {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    /// Item index of each `import` to the index of the module it loads.
    imports: HashMap<usize, usize>,
}

/// A module as discovered, before modules are put in execution order. `imports` and
/// `done`/edges below refer to other modules by *discovery* id, not by final [`ModId`]:
/// those only agree once [`Loader::finish`] has decided an order.
struct Discovered {
    path: String,
    file: u32,
    ast: Rc<ast::Module>,
    imports: HashMap<usize, usize>,
    /// Whether this module has any top-level statement (as opposed to only
    /// declarations), i.e. whether *when* it runs relative to other modules matters.
    runs_code: bool,
}

struct Loader<'h> {
    host: &'h mut dyn Host,
    sources: SourceMap,
    diags: Vec<Diagnostic>,
    nodes: Vec<Discovered>,
    /// Every `import` edge seen, as (importer, imported, span of the import path) — kept
    /// only to point at a concrete import if a cycle turns out to have no valid order.
    edges: Vec<(usize, usize, Span)>,
    done: HashMap<String, usize>,
    /// Node ids are unique across all modules.
    next_id: u32,
}

impl Loader<'_> {
    /// `key` identifies the module (see `Host::canonical_path`); `path` is how it is named.
    /// Discovers every module reachable from this one. A module's discovery id is
    /// reserved (in `done`) *before* its own imports are followed, so an import cycle
    /// (`a.q` and `b.q` importing each other) simply links back to the id already
    /// reserved instead of recursing forever or needing to be rejected here.
    fn discover(&mut self, path: &str, key: String, text: String) -> usize {
        let file = self.sources.add(path, &text);
        let (ast, next_id) = parse_module_from(&text, file, self.next_id, &mut self.diags);
        self.next_id = next_id;
        let runs_code = ast.items.iter().any(|it| matches!(it, Item::Stmt(_)));
        let id = self.nodes.len();
        self.nodes.push(Discovered {
            path: path.to_string(),
            file,
            ast: Rc::new(ast),
            imports: HashMap::new(),
            runs_code,
        });
        self.done.insert(key, id);

        let ast = self.nodes[id].ast.clone();
        let mut imports = HashMap::new();
        for (idx, item) in ast.items.iter().enumerate() {
            let Item::Import(imp) = item else { continue };
            let Some(target) = resolve_module_path(path, &imp.path) else {
                crate::diag::push_capped(
                    &mut self.diags,
                    Diagnostic::error(
                        "T100",
                        format!(
                            "cannot import \"{}\": use a path relative to this file, written with `/` (`./x.q`, `../lib/x.q`) or a package (`@name/x.q`), not an absolute one",
                            imp.path
                        ),
                        imp.path_span,
                    ),
                );
                continue;
            };
            let target = if crate::span::is_package_path(&target) {
                self.host.package_file(&target)
            } else {
                target
            };
            let target_key = self.host.canonical_path(&target);
            let m = if let Some(&m) = self.done.get(&target_key) {
                m
            } else {
                match self.host.load_module(&target) {
                    Ok(src) => self.discover(&target, target_key, src),
                    Err(e) => {
                        crate::diag::push_capped(
                            &mut self.diags,
                            Diagnostic::error(
                                "T102",
                                format!("cannot import \"{}\": {e}", imp.path),
                                imp.path_span,
                            ),
                        );
                        continue;
                    }
                }
            };
            imports.insert(idx, m);
            self.edges.push((id, m, imp.path_span));
        }
        self.nodes[id].imports = imports;
        id
    }

    /// Put the discovered modules in a valid execution order (a module after the
    /// modules it imports), rejecting only the cycles for which no such order exists:
    /// those where more than one module actually runs top-level code. Modules that
    /// only declare things (structs, fns, traits...) can be part of a cycle freely,
    /// since nothing about them depends on *when* they run.
    fn order(&mut self) -> Vec<usize> {
        let n = self.nodes.len();
        let succ: Vec<Vec<usize>> = (0..n)
            .map(|i| {
                let mut v: Vec<usize> = self.nodes[i].imports.values().copied().collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (u, outs) in succ.iter().enumerate() {
            for &v in outs {
                pred[v].push(u);
            }
        }

        // Kosaraju, step 1: finishing order of a DFS over the import graph.
        let mut visited = vec![false; n];
        let mut finish: Vec<usize> = Vec::with_capacity(n);
        for start in 0..n {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut stack = vec![(start, 0usize)];
            while let Some(&mut (u, ref mut i)) = stack.last_mut() {
                if *i < succ[u].len() {
                    let v = succ[u][*i];
                    *i += 1;
                    if !visited[v] {
                        visited[v] = true;
                        stack.push((v, 0));
                    }
                } else {
                    finish.push(u);
                    stack.pop();
                }
            }
        }

        // Step 2: strongly connected components, found on the reverse graph in
        // decreasing finishing time. This yields the SCCs in topological order of the
        // import graph (an importer's SCC before the SCCs it imports); reversed, that
        // becomes "a module's SCC comes after the SCCs it imports", which is what
        // execution order needs.
        let mut visited = vec![false; n];
        let mut sccs: Vec<Vec<usize>> = Vec::new();
        for &start in finish.iter().rev() {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut comp = vec![start];
            let mut stack = vec![start];
            while let Some(u) = stack.pop() {
                for &v in &pred[u] {
                    if !visited[v] {
                        visited[v] = true;
                        comp.push(v);
                        stack.push(v);
                    }
                }
            }
            comp.sort_unstable();
            sccs.push(comp);
        }
        sccs.reverse();

        for comp in &sccs {
            if comp.len() < 2 {
                continue;
            }
            let runs: Vec<usize> = comp
                .iter()
                .copied()
                .filter(|&m| self.nodes[m].runs_code)
                .collect();
            if runs.len() > 1 {
                let span = self
                    .edges
                    .iter()
                    .find(|&&(u, v, _)| comp.contains(&u) && comp.contains(&v))
                    .map(|&(_, _, s)| s)
                    .unwrap_or_default();
                let names: Vec<&str> = runs.iter().map(|&m| self.nodes[m].path.as_str()).collect();
                crate::diag::push_capped(
                    &mut self.diags,
                    Diagnostic::error(
                        "T101",
                        format!(
                            "these modules import each other in a cycle and each runs top-level code, so there is no valid order to run them: {}",
                            names.join(", ")
                        ),
                        span,
                    ),
                );
            }
        }

        sccs.into_iter().flatten().collect()
    }

    /// Consume the loader, deciding final module order and remapping every `imports`
    /// map from discovery ids to the resulting [`ModId`]s.
    fn finish(mut self) -> (SourceMap, Vec<Diagnostic>, Vec<Loaded>) {
        let order = self.order();
        let mut pos = vec![0usize; self.nodes.len()];
        for (new_id, &old_id) in order.iter().enumerate() {
            pos[old_id] = new_id;
        }
        let mut modules: Vec<Option<Loaded>> = (0..self.nodes.len()).map(|_| None).collect();
        for (old_id, node) in self.nodes.into_iter().enumerate() {
            let imports = node
                .imports
                .iter()
                .map(|(&idx, &target)| (idx, pos[target]))
                .collect();
            modules[pos[old_id]] = Some(Loaded {
                path: node.path,
                file: node.file,
                ast: node.ast,
                imports,
            });
        }
        let modules = modules
            .into_iter()
            .map(|m| m.expect("every position was assigned"))
            .collect();
        (self.sources, self.diags, modules)
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
    /// Variables some function or lambda of this file assigns (never narrowed).
    pub assigned_in_closures: HashSet<String>,
}

pub(crate) struct Checker {
    pub caps: Capabilities,
    /// Names of the `extern fun` this host does not provide.
    pub missing_externs: HashSet<String>,
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
            missing_externs: HashSet::new(),
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

    pub fn err_note(
        &mut self,
        code: &str,
        msg: impl Into<String>,
        span: Span,
        note: impl Into<String>,
        note_span: Option<Span>,
    ) {
        crate::diag::push_capped(
            &mut self.diags,
            Diagnostic::error(code, msg, span).with_note(note, note_span),
        );
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
            self.declare_items(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.declare_own_exports(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.resolve_imports(m);
        }
        for m in 0..n {
            self.cur_mod = m;
            self.declare_reexports(m);
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
            self.resolve_enums(m);
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
