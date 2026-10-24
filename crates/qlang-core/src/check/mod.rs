//! Type checker: loads modules, declares and resolves all definitions, then
//! checks every body. Its output is a [`Program`] ready to interpret.

mod annotations;
mod body;
mod calls;
mod captured;
mod catalog;
mod decls;
mod exhaust;
mod expr;
mod info;
mod unify;

pub use info::{ExprInfo, Info, ItemInfo, LocalInfo, Member, RefInfo};

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

--" The outcome of something that can fail: `Ok(value)` or `Err(error)`. Take it apart
with `match` (`case Ok(v) then ... case Err(e) then ...`), or with `is_ok()`, `unwrap()`...
Write `Ok(1)` / `Err(error)`, or `Result.Ok(1)` / `Result.Err(error)`. --"
enum Result<T, E>
    Ok(value: T)
    Err(error: E)
end

impl<T, E> Result<T, E>
    --" Whether this is an ok result. --"
    public fun is_ok(self) -> bool
        match self
            case Ok(_) then true
            case Err(_) then false
        end
    end

    --" Whether this is an error result. --"
    public fun is_err(self) -> bool
        match self
            case Ok(_) then false
            case Err(_) then true
        end
    end

    --" The ok value, or stops the program if this is an error result. --"
    public fun unwrap(self) -> T
        match self
            case Ok(v) then v
            case Err(_) then panic("called unwrap() on an error Result")
        end
    end

    --" The error value, or stops the program if this is an ok result. --"
    public fun unwrap_err(self) -> E
        match self
            case Ok(_) then panic("called unwrap_err() on an ok Result")
            case Err(e) then e
        end
    end
end

--" An ok `Result`, usually inferred from context (`return Ok(1)` inside a function
returning `Result<int, Error>`, `let r: Result<int, Error> = Ok(1)`...). When the
compiler cannot infer `E` from anywhere, write `Result<int, Error>.Ok(1)` directly. --"
fun Ok<T, E>(value: T) -> Result<T, E>
    Result<T, E>.Ok(value)
end

--" An error `Result`: see `Ok`. --"
fun Err<T, E>(error: E) -> Result<T, E>
    Result<T, E>.Err(error)
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

impl ToJson for Json
    public fun to_json(self) -> string
        self.to_text()
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

-- ===== reflection =====

--" An annotation as reflection reads it: the `name` of its struct (`Route`) and its arguments
by field name, as JSON values (numbers are floats). --"
struct AnnotationInfo
    public name: string
    public args: map<string, Json>
end

impl AnnotationInfo
    --" The text argument `field`, or `none` (no such argument, or not a string). --"
    public fun get_string(self, field: string) -> string?
        let j = self.args[field]
        if j == none or not j.is_string() then none else j.as_string() end
    end

    --" The integer argument `field`, or `none`. --"
    public fun get_int(self, field: string) -> int?
        let j = self.args[field]
        if j == none or not j.is_number() then none else j.as_number() as int end
    end

    --" The number argument `field`, or `none`. --"
    public fun get_float(self, field: string) -> float?
        let j = self.args[field]
        if j == none or not j.is_number() then none else j.as_number() end
    end

    --" The bool argument `field`, or `none`. --"
    public fun get_bool(self, field: string) -> bool?
        let j = self.args[field]
        if j == none or not j.is_bool() then none else j.as_bool() end
    end
end

--" A parameter of a function or a variant, as reflection describes it. --"
struct ParamInfo
    public name: string
    public type_name: string
    public annotations: array<AnnotationInfo>
    ty: Any
end

impl ParamInfo
    --" Reads `json` as the type of this parameter (`Json.to<T>` when `T` is only known by
    reflection). The `Any` holds a value of that type. --"
    public fun decode(self, json: Json) -> Result<Any, Error>
        Reflect.decode(self.ty, json)
    end

    --" Reads `text` (a path segment, a query parameter) as the type of this parameter: `int`,
    `float`, `bool`, `string`, `uuid`, `ulid`, `datetime`, or one of them followed by `?` (empty
    text is then `none`). --"
    public fun parse(self, text: string) -> Result<Any, Error>
        Reflect.parse_text(self.ty, text)
    end

    --" The shape of the type of this parameter. --"
    public fun type_info(self) -> TypeInfo
        Reflect.type_info_of(self.ty)
    end
end

--" The shape of a type, as `Reflect.type_info<T>()` describes it: enough to write a schema. --"
struct TypeInfo
    --" `int`, `float`, `bool`, `string`, `uuid`, `ulid`, `datetime`, `bytes`, `any`, `unit`,
    `json`, `array`, `map`, `tuple`, `struct`, `enum`, `function` or `other`. A `T?` has the kind
    of `T` and `nullable` set. --"
    public kind: string
    --" The name of a struct or an enum, else the type as written. --"
    public name: string
    public nullable: bool
    --" The element of an array, the value of a map. --"
    public item: TypeInfo?
    --" The fields `Json.of` writes: public, not `#[JsonSkip]`, under their `#[JsonName]`. --"
    public fields: array<FieldType>
    --" The variants of an enum. --"
    public variants: array<string>
    --" The type arguments of a generic struct or enum: `Result<Post, HttpError>` has two. --"
    public args: array<TypeInfo>
    --" Set on a struct met again inside itself: its fields are left out there. --"
    public recursive: bool
end

--" A field of a struct for a schema: its name in JSON, its shape and its annotations. --"
struct FieldType
    --" The name in JSON. --"
    public name: string
    --" The name of the field. --"
    public field: string
    public type_name: string
    public annotations: array<AnnotationInfo>
    public info: TypeInfo
end

impl FieldType
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

--" A field of a struct, as reflection describes it. --"
struct FieldInfo
    public name: string
    public type_name: string
    public annotations: array<AnnotationInfo>
end

--" A struct, as `Reflect.struct_info<T>()` describes it. --"
struct StructInfo
    public name: string
    public fields: array<FieldInfo>
    public annotations: array<AnnotationInfo>
end

--" A variant of an enum, as reflection describes it. --"
struct VariantInfo
    public name: string
    public fields: array<ParamInfo>
    public annotations: array<AnnotationInfo>
end

--" An enum, as `Reflect.enum_info<T>()` describes it. --"
struct EnumInfo
    public name: string
    public variants: array<VariantInfo>
    public annotations: array<AnnotationInfo>
end

--" A function, as `Reflect.functions()` describes it. --"
struct FunctionInfo
    public name: string
    public has_self: bool
    public params: array<ParamInfo>
    public return_type: string
    public annotations: array<AnnotationInfo>
end

--" The annotation called `name` in `list`, or `none`. --"
fun reflect_find_annotation(list: array<AnnotationInfo>, name: string) -> AnnotationInfo?
    for a in list do
        if a.name == name then
            return a
        end
    end
    none
end

impl ParamInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl FieldInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl StructInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl VariantInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl EnumInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

impl FunctionInfo
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end
end

--" The inside of an enumeration value, as `Reflect.parts` gives it: the name of the enum, of its
variant and the data it carries (each as an `Any` with its own type). --"
struct EnumParts
    public enum_name: string
    public variant: string
    public values: array<Any>
end

--" A function found by `Reflect.callable(name)` (or `Reflect.callable(module, name)`), to call with
`Any` values: `call` checks their number and types against `params`. --"
struct Callable
    public name: string
    public params: array<ParamInfo>
    public return_type: string
    public annotations: array<AnnotationInfo>
    target: Any
    ret: Any
end

impl Callable
    --" The annotation called `name`, or `none`. --"
    public fun annotation(self, name: string) -> AnnotationInfo?
        reflect_find_annotation(self.annotations, name)
    end

    --" Calls the function. `err` when the number of arguments or one type is wrong. --"
    public fun call(self, args: array<Any>) -> Result<Any, Error>
        Reflect.invoke(self.target, args)
    end

    --" The shape of what the function returns. --"
    public fun return_info(self) -> TypeInfo
        Reflect.type_info_of(self.ret)
    end
end

--" Names a field differently in `Json.of` and `Json.to`: `#[JsonName("user_id")]`. --"
#[annotation]
struct JsonName
    name: string
end

--" Leaves a field out of `Json.of`. --"
#[annotation]
struct JsonSkip
end

--" Reflection: `Reflect.struct_info<T>()`, `Reflect.enum_info<T>()`, `Reflect.functions()` (the functions
of this file) and `Reflect.find<fun(A) -> B>("name")` (a function of this file, if its type is
that one); `Reflect.methods<T>()` and `Reflect.find_method<T, F>(value, "name")` do the same
for the methods of a type. --"
struct Reflect
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
    --" The JSON text of this value: `{"a":1,"b":[true,null]}`. --"
    public fun to_text(self) -> string
        Reflect.json_text(self)
    end

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
        Result<Json, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: {message}" })
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
            Result<Json, Error>.Ok(Json.boolean(true))
        elseif self.looking_at("false") then
            self.pos = self.pos + 5
            Result<Json, Error>.Ok(Json.boolean(false))
        elseif self.looking_at("null") then
            self.pos = self.pos + 4
            Result<Json, Error>.Ok(Json.null())
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
                return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: unterminated string" })
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
                        return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: invalid \\u escape" })
                    end
                    out += code.char()
                    self.pos = self.pos + 4
                else
                    return Result<string, Error>.Err(Error { message: "JSON parse error at offset {self.pos}: invalid escape \\{e}" })
                end
                self.advance()
            else
                out += self.peek()
                self.advance()
            end
        end
        self.advance()
        Result<string, Error>.Ok(out)
    end

    public fun parse_string_value(self) -> Result<Json, Error>
        let r = self.parse_raw_string()
        if r.is_err() then
            return Result<Json, Error>.Err(r.unwrap_err())
        end
        Result<Json, Error>.Ok(Json.string(r.unwrap()))
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
            return Result<Json, Error>.Err(Error { message: "JSON parse error at offset {start}: invalid number" })
        end
        Result<Json, Error>.Ok(Json.number(n))
    end

    public fun parse_array(self) -> Result<Json, Error>
        self.advance()
        let items: array<Json> = []
        self.skip_ws()
        if self.peek() == "]" then
            self.advance()
            return Result<Json, Error>.Ok(Json.array(items))
        end
        while true do
            let r = self.parse_value()
            if r.is_err() then
                return Result<Json, Error>.Err(r.unwrap_err())
            end
            items[] = r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "]" then
                self.advance()
                return Result<Json, Error>.Ok(Json.array(items))
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
            return Result<Json, Error>.Ok(Json.object(fields))
        end
        while true do
            self.skip_ws()
            if self.peek() != "\"" then
                return self.fail("expected a string key")
            end
            let key_r = self.parse_raw_string()
            if key_r.is_err() then
                return Result<Json, Error>.Err(key_r.unwrap_err())
            end
            self.skip_ws()
            if self.peek() != ":" then
                return self.fail("expected ':'")
            end
            self.advance()
            let value_r = self.parse_value()
            if value_r.is_err() then
                return Result<Json, Error>.Err(value_r.unwrap_err())
            end
            fields[key_r.unwrap()] = value_r.unwrap()
            self.skip_ws()
            if self.peek() == "," then
                self.advance()
            elseif self.peek() == "\}" then
                self.advance()
                return Result<Json, Error>.Ok(Json.object(fields))
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
            return Result<Json, Error>.Err(trailing)
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
    --" The body as bytes, for a body that is not text (an upload, an image). For a received
    request it is the exact body; for `http.send` it wins over `body`. --"
    public raw: bytes? = none
    public path: string = ""
    public query: map<string, string> = {}
    public params: map<string, string> = {}
    --" Every value of each query parameter, in order (`?a=1&a=2` gives `{"a": ["1", "2"]}`). --"
    public query_all: map<string, array<string>> = {}
    --" Every value of each header (lower-case names): a header sent twice has two values. --"
    public headers_all: map<string, array<string>> = {}
    --" The cookies of the `Cookie` header, with their values decoded. --"
    public cookies: map<string, string> = {}
    --" What the program attaches to this request while it handles it (the user an
    authentication step found, for example); read back with `as<T>()`. --"
    public attrs: map<string, Any> = {}
end

--" One part of a `multipart/form-data` body: a field of the form or a file. --"
struct Part
    public name: string
    --" The name of the uploaded file, or `none` for an ordinary field. --"
    public filename: string? = none
    public content_type: string? = none
    public data: bytes
end

impl Part
    --" The content as text, or `none` if it is not valid text. --"
    public fun text(self) -> string?
        self.data.to_string()
    end
end

fun multipart_param(header: string, key: string) -> string?
    let found = regex.captures(header, "(?i)(?:^|[;\\s])" + key + "=\"([^\"]*)\"")
    if found == none then
        return none
    end
    found[1]
end

fun multipart_boundary(content_type: string) -> string?
    let found = regex.captures(content_type, "(?i)boundary=(?:\"([^\"]+)\"|([^;\\s]+))")
    if found == none then
        return none
    end
    let quoted = found[1]
    if quoted != none then
        return quoted
    end
    found[2]
end

fun multipart_error(message: string) -> Result<array<Part>, Error>
    Result<array<Part>, Error>.Err(Error { message: message })
end

impl Request
    --" The parts of a `multipart/form-data` body (an HTML form with files): `err` when the request
    is not one or its body is malformed. The files are in `Part.data`. --"
    public fun multipart(self) -> Result<array<Part>, Error>
        let kind = self.header("content-type")
        if kind == none or not kind.lower().starts_with("multipart/form-data") then
            return multipart_error("the body is not multipart/form-data")
        end
        let boundary = multipart_boundary(kind)
        let raw = self.raw
        if boundary == none or raw == none then
            return multipart_error("the multipart body has no boundary or no content")
        end
        let delim = "--{boundary}".to_bytes()
        let crlf = "\r\n".to_bytes()
        let blank = "\r\n\r\n".to_bytes()
        let after = "\r\n--{boundary}".to_bytes()
        let first = raw.find(delim)
        if first == none then
            return multipart_error("the multipart body does not start with its boundary")
        end
        let parts: array<Part> = []
        let pos = first + delim.len()
        let total = raw.len()
        while true do
            let rest = raw.slice(pos, total)
            if rest.starts_with("--".to_bytes()) then
                break
            end
            if not rest.starts_with(crlf) then
                return multipart_error("a multipart boundary is not followed by a line break")
            end
            let body = rest.slice(2, rest.len())
            let head_end = body.find(blank)
            if head_end == none then
                return multipart_error("a multipart part has no end of headers")
            end
            let head_text = body.slice(0, head_end).to_string()
            if head_text == none then
                return multipart_error("the headers of a multipart part are not text")
            end
            let content = body.slice(head_end + 4, body.len())
            let stop = content.find(after)
            if stop == none then
                return multipart_error("a multipart part is not closed by a boundary")
            end
            let disposition = ""
            let content_type: string? = none
            for line in head_text.split("\r\n") do
                let lowered = line.lower()
                if lowered.starts_with("content-disposition:") then
                    disposition = line
                end
                if lowered.starts_with("content-type:") then
                    content_type = line.substring(13, line.len()).trim()
                end
            end
            let name = multipart_param(disposition, "name")
            if name == none then
                return multipart_error("a multipart part has no name")
            end
            parts.push(Part { name: name, filename: multipart_param(disposition, "filename"), content_type: content_type, data: content.slice(0, stop) })
            pos = total - content.len() + stop + 2 + delim.len()
        end
        Result<array<Part>, Error>.Ok(parts)
    end

    --" The values of the header `name` (whatever the case of `name`); empty when there is none. --"
    public fun header_all(self, name: string) -> array<string>
        let found = self.headers_all[name.lower()]
        if found == none then [] else found end
    end

    --" The fields of a body sent as `application/x-www-form-urlencoded`, decoded. `err` when
    the request is not a form. --"
    public fun form(self) -> Result<map<string, string>, Error>
        let kind = self.header("content-type")
        if kind == none or not kind.lower().starts_with("application/x-www-form-urlencoded") then
            return Result<map<string, string>, Error>.Err(Error { message: "the body is not a form (application/x-www-form-urlencoded)" })
        end
        let out: map<string, string> = {}
        let body = self.body
        if body == none then
            return Result<map<string, string>, Error>.Ok(out)
        end
        for pair in body.split("&") do
            if pair == "" then
                continue
            end
            let parts = pair.split("=")
            let key = Url.decode(parts[0])
            let value = Url.decode(parts.slice(1, parts.len()).join("="))
            if key == none or value == none then
                return Result<map<string, string>, Error>.Err(Error { message: "the form has a badly encoded field" })
            end
            out[key] = value
        end
        Result<map<string, string>, Error>.Ok(out)
    end

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
    --" The exact bytes of the body when it is not text; `body` is then a lossy text. A
    program that answers with `raw` set sends these bytes instead of `body`. --"
    public raw: bytes? = none
    --" The cookies to set: one `Set-Cookie` header each. --"
    public cookies: array<Cookie> = []
    --" A body produced piece by piece: the server calls this function again and again and sends
    each text it gives, until it gives `none`. The worker that answers is busy until then. --"
    public stream: (fun() -> string?)? = none
    --" Turns the connection into a WebSocket (see `Response.websocket`). --"
    public websocket: WebSocketHandler? = none
end

--" A message received on a WebSocket: `text` for a text message, `data` for a binary one. --"
struct WsMessage
    public text: string? = none
    public data: bytes? = none
    public binary: bool = false
end

--" One thing a WebSocket has to send; `WebSocket.send` and friends make them. --"
struct WsFrame
    --" `text`, `binary` or `close`. --"
    public kind: string
    public text: string = ""
    public data: bytes = bytes.empty()
    public code: int = 1000
end

--" One WebSocket connection, as the callbacks of a `WebSocketHandler` see it. Many connections
live in the same worker: do not wait inside a callback. --"
struct WebSocket
    --" A unique identifier of this connection. --"
    public id: string
    --" The request that opened it (path, query, cookies, what a guard put in `attrs`). --"
    public request: Request
    --" Whatever you want to remember about this connection (who it is, the room it joined). --"
    public data: map<string, Any> = {}
    outbox: array<WsFrame> = []
    open: bool = true
end

impl WebSocket
    --" Sends a text message. --"
    public fun send(self, text: string)
        if self.open then
            self.outbox.push(WsFrame { kind: "text", text: text })
        end
    end

    --" Sends a binary message. --"
    public fun send_bytes(self, data: bytes)
        if self.open then
            self.outbox.push(WsFrame { kind: "binary", data: data })
        end
    end

    --" Closes the connection (code 1000: normal). --"
    public fun close(self)
        self.close_with(1000, "")
    end

    --" Closes the connection with a code (1000-4999) and a short reason. --"
    public fun close_with(self, code: int, reason: string)
        if self.open then
            self.open = false
            self.outbox.push(WsFrame { kind: "close", text: reason, code: code })
        end
    end

    public fun is_open(self) -> bool
        self.open
    end

    --" What was queued to send since the last call, and forgets it: how a test reads what a
    callback wrote (the server does the same before sending). --"
    public fun take_frames(self) -> array<WsFrame>
        let out = self.outbox
        self.outbox = []
        out
    end
end

--" What happens on a WebSocket: each function is called with the connection, one event at a time
(a worker serves many connections, so a callback must be short). --"
struct WebSocketHandler
    --" A message arrived. --"
    public on_message: fun(WebSocket, WsMessage)
    --" The connection was opened. --"
    public on_open: (fun(WebSocket))? = none
    --" Called every `tick_ms` while the connection is open: poll a database, send the news. --"
    public on_tick: (fun(WebSocket))? = none
    public tick_ms: int = 1000
    --" The connection ended (the client left, or `close`). Nothing can be sent any more. --"
    public on_close: (fun(WebSocket))? = none
end

--" One event of a server-sent events stream (`Response.sse`). --"
struct SseEvent
    public data: string
    public event: string? = none
    public id: string? = none
    public retry: int? = none
end

impl SseEvent
    --" The event as the browser reads it: `event:`, `id:`, `retry:` and a `data:` line for each
    line of `data`, then an empty line. --"
    public fun to_text(self) -> string
        let out = ""
        if self.event != none then
            out += "event: {self.event}\n"
        end
        if self.id != none then
            out += "id: {self.id}\n"
        end
        if self.retry != none then
            out += "retry: {self.retry}\n"
        end
        for line in self.data.split("\n") do
            out += "data: {line}\n"
        end
        out + "\n"
    end
end

--" A cookie to send with a `Response`. `value` is sent percent-encoded (`Request.cookies` decodes
it); `max_age` is in seconds (`none`: a cookie for the session); `same_site` is `"Lax"`,
`"Strict"` or `"None"`. --"
struct Cookie
    public name: string
    public value: string
    public path: string = "/"
    public domain: string? = none
    public max_age: int? = none
    public secure: bool = false
    public http_only: bool = true
    public same_site: string = "Lax"
end

--" An error that has an HTTP status: what a handler gives back when it wants more than a 500. --"
struct HttpError
    public status: int
    public message: string
end

impl HttpError
    --" The response for this error: its message, as text, with its status. --"
    public fun to_response(self) -> Response
        Response { status: self.status, headers: { "Content-Type": "text/plain; charset=utf-8" }, body: self.message }
    end
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

    --" A response whose body is produced piece by piece: the server calls `next` again and again,
    sends each text it gives (right away) and ends when it gives `none`. For long answers and
    events. While it runs, the worker that answers serves only this connection (give
    `--workers` enough). --"
    public fun stream(content_type: string, next: fun() -> string?) -> Response
        Response { status: 200, headers: { "Content-Type": content_type, "Cache-Control": "no-cache", "X-Accel-Buffering": "no" }, body: "", stream: next }
    end

    --" A server-sent events response (`text/event-stream`): `next` gives the events one after the
    other, waiting (`time.sleep`) between them if it wants; `none` ends the stream. --"
    public fun sse(next: fun() -> SseEvent?) -> Response
        Response.stream("text/event-stream", fun() -> string?
            let e = next()
            if e == none then none else e.to_text() end
        end)
    end

    --" Turns the request into a WebSocket (the request must ask for it: `Upgrade: websocket`).
    From then on `handler` is called for each event of the connection. --"
    public fun websocket(handler: WebSocketHandler) -> Response
        Response { status: 101, body: "", websocket: handler }
    end

    --" A response (status 200) with these bytes as its body and this `Content-Type`. --"
    public fun bytes(data: bytes, content_type: string) -> Response
        Response { status: 200, headers: { "Content-Type": content_type }, body: "", raw: data }
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

    --" This response with a cookie more (returns the response itself). --"
    public fun with_cookie(self, cookie: Cookie) -> Response
        self.cookies.push(cookie)
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
        Result.Ok(true)
    else
        Result.Err(r.unwrap_err())
    end
end

-- The host provides the disk, the process environment and the network too.
extern fun fs_read(path: string) -> Result<string, Error>
extern fun fs_write(path: string, content: string) -> Result<bool, Error>
extern fun fs_read_bytes(path: string) -> Result<bytes, Error>
extern fun fs_write_bytes(path: string, content: bytes) -> Result<bool, Error>
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

    --" Reads a file's whole content as bytes (an image, an archive). Needs disk access. --"
    public fun read_bytes(path: string) -> Result<bytes, Error>
        fs_read_bytes(path)
    end

    --" Writes bytes to a file, replacing it entirely (creating it if needed). Needs disk access. --"
    public fun write_bytes(path: string, content: bytes) -> Result<bool, Error>
        fs_write_bytes(path, content)
    end
end

-- The host provides secure randomness and password hashing.
extern fun secure_random_bytes(n: int) -> Result<bytes, Error>
extern fun constant_time_eq(a: bytes, b: bytes) -> bool
extern fun password_hash(plain: string) -> Result<string, Error>
extern fun password_verify(plain: string, hash: string) -> Result<bool, Error>
extern fun password_needs_rehash(hash: string) -> Result<bool, Error>

-- The host writes the program's log (every host has this one).
extern fun log_write(level: string, message: string, fields: map<string, string>) -> bool

--" The program's log: `Log.info("started")`, with the levels `debug`, `info`, `warn` and
`error`. The `qlang` command writes each line on its standard error (`--log-level warn` hides the
lower ones, `--log-format json` writes JSON); another host may keep the lines or drop them. --"
struct Log
end

impl Log
    --" Writes `message` at the level `level` with extra `name: value` pairs (`"debug"`, `"info"`,
    `"warn"` or `"error"`; any other level is written as `info`). --"
    public fun write(level: string, message: string, fields: map<string, string>)
        let known = level == "debug" or level == "info" or level == "warn" or level == "error"
        log_write(if known then level else "info" end, message, fields)
    end

    --" A detail for whoever looks for a problem. --"
    public fun debug(message: string)
        log_write("debug", message, {})
    end

    --" Something that happened and is worth knowing. --"
    public fun info(message: string)
        log_write("info", message, {})
    end

    --" Something odd that the program survives. --"
    public fun warn(message: string)
        log_write("warn", message, {})
    end

    --" Something that failed. --"
    public fun error(message: string)
        log_write("error", message, {})
    end
end

--" Secrets: random bytes nobody can guess, and a comparison that does not leak where two
secrets differ. Needs a source of secure randomness (the `qlang` command has one). --"
struct Crypto
end

impl Crypto
    --" `n` bytes (0 to 1048576) from the operating system's secure source: never the seeded
    generator of `random`. --"
    public fun random_bytes(n: int) -> Result<bytes, Error>
        secure_random_bytes(n)
    end

    --" A token for a session, a reset link or an API key: `n` secure random bytes written in
    URL-safe base64 (32 bytes give 43 characters). --"
    public fun token(n: int) -> Result<string, Error>
        let r = secure_random_bytes(n)
        if r.is_err() then
            return Result<string, Error>.Err(r.unwrap_err())
        end
        Result<string, Error>.Ok(Base64.encode_url_bytes(r.unwrap()))
    end

    --" `true` if the two texts are equal. It takes the same time wherever they differ, so it is the
    way to compare a token, a signature or a hash with one received. --"
    public fun equal(a: string, b: string) -> bool
        constant_time_eq(a.to_bytes(), b.to_bytes())
    end

    --" `true` if `signature` is the signature of `message` made with the private key that goes with
    `key`, a public key as a PEM (`BEGIN PUBLIC KEY`, `BEGIN RSA PUBLIC KEY`) or a JWK (JSON text).
    `alg` is `RS256`, `RS384`, `RS512` (RSA PKCS#1), `ES256`, `ES384` (the signature is `r` then `s`, as a
    JWT writes it) or `EdDSA` (Ed25519). A wrong signature is `false`; `err` is for a key or an algorithm
    that cannot be used. Needs the `qlang` command. --"
    public fun verify(alg: string, key: string, message: bytes, signature: bytes) -> Result<bool, Error>
        crypto_verify(alg, key, message, signature)
    end

    --" The signature of `message` with a private key: a PEM in PKCS#8 (`BEGIN PRIVATE KEY`; for RSA
    also `BEGIN RSA PRIVATE KEY`). Same algorithms as `verify`. Needs the `qlang` command. --"
    public fun sign(alg: string, key: string, message: bytes) -> Result<bytes, Error>
        crypto_sign(alg, key, message)
    end

    --" Like `equal` for bytes. --"
    public fun equal_bytes(a: bytes, b: bytes) -> bool
        constant_time_eq(a, b)
    end
end

--" Passwords: `Password.hash` makes a text to store (Argon2id, with its own random salt and
settings inside), `Password.verify` checks a password against it. Never store the password
itself. Only the `qlang` command offers it. --"
struct Password
end

impl Password
    --" The hash of `plain`, to keep in the database. Slow on purpose (tens of milliseconds). --"
    public fun hash(plain: string) -> Result<string, Error>
        password_hash(plain)
    end

    --" `true` if `plain` is the password `hash` was made from; `err` if `hash` is not a hash. --"
    public fun verify(plain: string, hash: string) -> Result<bool, Error>
        password_verify(plain, hash)
    end

    --" `true` if `hash` was made with weaker settings than today's: hash the password again, after
    a successful `verify`, and store the new one. --"
    public fun needs_rehash(hash: string) -> Result<bool, Error>
        password_needs_rehash(hash)
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

--" Stops the program (a failed test) unless `actual < limit`, showing both values. --"
fun assert_lt<T: Ord>(actual: T, limit: T)
    if not (actual < limit) then
        panic("assert_lt failed: {actual} is not less than {limit}")
    end
end

--" Stops the program (a failed test) unless `actual <= limit`, showing both values. --"
fun assert_le<T: Ord>(actual: T, limit: T)
    if not (actual <= limit) then
        panic("assert_le failed: {actual} is not less than or equal to {limit}")
    end
end

--" Stops the program (a failed test) unless `actual > limit`, showing both values. --"
fun assert_gt<T: Ord>(actual: T, limit: T)
    if not (actual > limit) then
        panic("assert_gt failed: {actual} is not greater than {limit}")
    end
end

--" Stops the program (a failed test) unless `actual >= limit`, showing both values. --"
fun assert_ge<T: Ord>(actual: T, limit: T)
    if not (actual >= limit) then
        panic("assert_ge failed: {actual} is not greater than or equal to {limit}")
    end
end

--" Stops the program (a failed test) unless `items` has `expected` elements. --"
fun assert_len<T>(items: array<T>, expected: int)
    if items.len() != expected then
        panic("assert_len failed: {items} has {items.len()} element(s), expected {expected}")
    end
end

--" Stops the program (a failed test) unless `items` has no element. --"
fun assert_empty<T>(items: array<T>)
    if items.len() != 0 then
        panic("assert_empty failed: got {items}")
    end
end

--" Stops the program (a failed test) if `item` is in `items`. --"
fun assert_not_contains<T: Eq>(items: array<T>, item: T)
    if items.contains(item) then
        panic("assert_not_contains failed: {items} contains {item}")
    end
end

--" Stops the program (a failed test) unless `text` starts with `prefix`. --"
fun assert_starts_with(text: string, prefix: string)
    if not text.starts_with(prefix) then
        panic("assert_starts_with failed: \"{text}\" does not start with \"{prefix}\"")
    end
end

--" Stops the program (a failed test) unless `text` ends with `suffix`. --"
fun assert_ends_with(text: string, suffix: string)
    if not text.ends_with(suffix) then
        panic("assert_ends_with failed: \"{text}\" does not end with \"{suffix}\"")
    end
end

--" A set: values without duplicates, kept in the order they were added. The values follow
the rules of map keys (`int`, `float`, `string`, `bool`, `uuid`, `ulid`, `datetime`, an enum
whose variants carry no data). Make one with `Set<int>.new()` or `Set.of([1, 2, 2, 3])`,
go through it with `for x in s`. --"
struct Set<T>
    items: map<T, bool>
end

impl<T> Set<T>
    --" An empty set: `Set<int>.new()`. --"
    public fun new() -> Set<T>
        let m: map<T, bool> = {}
        Set { items: m }
    end

    --" A set with the values of `values`, without the duplicates. --"
    public fun of(values: array<T>) -> Set<T>
        let s = Set<T>.new()
        for v in values do
            s.add(v)
        end
        s
    end

    --" Adds `value`; whether it was new. --"
    public fun add(self, value: T) -> bool
        if self.items.has(value) then
            return false
        end
        self.items[value] = true
        true
    end

    --" Whether `value` is in the set. --"
    public fun contains(self, value: T) -> bool
        self.items.has(value)
    end

    --" Removes `value`; whether it was there. --"
    public fun remove(self, value: T) -> bool
        self.items.remove(value) != none
    end

    --" The number of values. --"
    public fun len(self) -> int
        self.items.len()
    end

    --" Whether the set has no value. --"
    public fun is_empty(self) -> bool
        self.items.len() == 0
    end

    --" Removes every value. --"
    public fun clear(self)
        self.items.clear()
    end

    --" The values, in the order they were added. --"
    public fun to_array(self) -> array<T>
        self.items.keys()
    end

    --" A new set with the values of both sets. --"
    public fun union(self, other: Set<T>) -> Set<T>
        let out = Set<T>.of(self.to_array())
        for v in other do
            out.add(v)
        end
        out
    end

    --" A new set with the values that are in both sets. --"
    public fun intersect(self, other: Set<T>) -> Set<T>
        let out = Set<T>.new()
        for v in self do
            if other.contains(v) then
                out.add(v)
            end
        end
        out
    end

    --" A new set with the values of this set that are not in `other`. --"
    public fun difference(self, other: Set<T>) -> Set<T>
        let out = Set<T>.new()
        for v in self do
            if not other.contains(v) then
                out.add(v)
            end
        end
        out
    end

    --" Whether every value of this set is in `other`. --"
    public fun is_subset(self, other: Set<T>) -> bool
        for v in self do
            if not other.contains(v) then
                return false
            end
        end
        true
    end

    --" Whether both sets have the same values (whatever their order). --"
    public fun equals(self, other: Set<T>) -> bool
        self.len() == other.len() and self.is_subset(other)
    end
end

impl<T> As<string> for Set<T>
    public fun convert(self) -> string
        let parts: array<string> = []
        for v in self do
            parts[] = "{v}"
        end
        let body = parts.join(", ")
        "Set({body})"
    end
end

--" The value of an ok `Result`; stops the program (a failed test) showing the error otherwise. --"
fun assert_ok<T, E>(result: Result<T, E>) -> T
    match result
        case Ok(v) then v
        case Err(e) then panic("assert_ok failed: got the error {e}")
    end
end

--" The error of an error `Result`; stops the program (a failed test) showing the value if it is ok. --"
fun assert_err<T, E>(result: Result<T, E>) -> E
    match result
        case Ok(v) then panic("assert_err failed: got the ok value {v}")
        case Err(e) then e
    end
end

--" The value, which must not be `none`; stops the program (a failed test) otherwise. --"
fun assert_some<T>(value: T?) -> T
    let v = value
    if v != none then
        v
    else
        panic("assert_some failed: the value is none")
    end
end

--" Stops the program (a failed test) unless the value is `none`. --"
fun assert_none<T>(value: T?)
    if value != none then
        panic("assert_none failed: got {value}")
    end
end

--" Stops the program (a failed test) unless `item` is in `items`. --"
fun assert_contains<T: Eq>(items: array<T>, item: T)
    if not items.contains(item) then
        panic("assert_contains failed: {items} does not contain {item}")
    end
end

--" Stops the program (a failed test) unless `text` contains `part`. --"
fun assert_contains_text(text: string, part: string)
    if not text.contains(part) then
        panic("assert_contains_text failed: \"{text}\" does not contain \"{part}\"")
    end
end

--" Stops the program (a failed test) unless `actual` is within `tolerance` of `expected`
(relative to `expected` once it is larger than 1): for numbers that went through float
arithmetic. --"
fun assert_within(actual: float, expected: float, tolerance: float)
    let scale = expected.abs().max(1.0)
    if (actual - expected).abs() > tolerance * scale then
        panic("assert_within failed: {actual} is not within {tolerance} of {expected}")
    end
end

--" Like `assert_within` with a tolerance of one billionth. --"
fun assert_close(actual: float, expected: float)
    assert_within(actual, expected, 0.000000001)
end

-- Signatures with a key pair: only the `qlang` command offers them.
extern fun crypto_verify(alg: string, key: string, message: bytes, signature: bytes) -> Result<bool, Error>
extern fun crypto_sign(alg: string, key: string, message: bytes) -> Result<bytes, Error>

-- Whether the process was asked to stop (Ctrl-C, `kill`): only the `qlang` command.
extern fun process_stopping() -> bool

-- Starting programs: only where the host allows it.
extern fun process_run(command: string, args: array<string>, input: string?) -> Result<ProcessOutput, Error>

--" What a program wrote on its two output streams, and the code it ended with (-1 when a
signal ended it). --"
struct ProcessOutput
    public status: int
    public stdout: string
    public stderr: string
end

--" Starting programs: `Process.run("git", ["status"])`. No shell is involved, so the
arguments are passed as they are. Needs permission (`--allow-process` in the terminal); a
program is not confined by `--root`, and one that runs for more than a minute is stopped. --"
struct Process
end

impl Process
    --" Whether the program was asked to stop (Ctrl-C, `kill`, a service manager). The first call
    makes the program take over: from then on the signal no longer ends it at once, it only makes
    this return `true`, so that a worker can finish what it is doing and leave. A second signal
    ends the program for good. In a `qlang serve` program it is `true` while the server stops.
    Needs the `qlang` command. --"
    public fun stopping() -> bool
        process_stopping()
    end

    --" Runs `command` with `args` and waits for it. `err` if it cannot be started; a program
    that ends with a non-zero `status` is still an `ok` result. --"
    public fun run(command: string, args: array<string>) -> Result<ProcessOutput, Error>
        process_run(command, args, none)
    end

    --" Like `run`, with `input` written to the program's standard input. --"
    public fun run_with_input(command: string, args: array<string>, input: string) -> Result<ProcessOutput, Error>
        process_run(command, args, input)
    end
end

-- The `qlang` command provides Redis: these are the `extern fun` it implements.
extern fun redis_connect(url: string) -> Result<int, Error>
extern fun redis_command(conn: int, args: array<string>) -> Result<Json, Error>
extern fun redis_poll(conn: int, timeout_ms: int) -> Result<Json, Error>
extern fun redis_close(conn: int) -> Result<bool, Error>

--" A message received on a Redis subscription. --"
struct RedisMessage
    public channel: string
    public payload: string
    --" The pattern that matched, for a `psubscribe`. --"
    public pattern: string? = none
end

--" A Redis connection, opened with `Redis.connect(url)`: `redis://[user:password@]host[:port][/db]`, or
`rediss://` for TLS. A server that is not on this machine needs `rediss://` (or `?tls=disable` to say
that you want it in clear). Every command waits for its answer (30 s at most), so a connection serves
one thing at a time: use one connection for commands and another for a subscription. --"
struct Redis
    --" The number of the connection, for the host. --"
    public conn: int
end

impl Redis
    --" Opens a connection. Needs the `qlang` command. --"
    public fun connect(url: string) -> Result<Redis, Error>
        let r = redis_connect(url)
        if r.is_ok() then
            Result.Ok(Redis { conn: r.unwrap() })
        else
            Result.Err(r.unwrap_err())
        end
    end

    --" Opens a connection that only listens: it is subscribed to these channels, and `next` reads what
    is published. --"
    public fun subscriber(url: string, channels: array<string>) -> Result<Subscription, Error>
        let r = redis_connect(url)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let sub = Subscription { conn: r.unwrap() }
        if channels.is_empty() then
            return Result.Ok(sub)
        end
        let done = sub.subscribe(channels)
        if done.is_err() then
            sub.close()
            return Result.Err(done.unwrap_err())
        end
        Result.Ok(sub)
    end

    --" Any command, as its words: `r.command(["SET", "k", "v"])`. The answer is a `Json`: a text, a
    number, `null`, or an array of those. A Redis error is an `err`. --"
    public fun command(self, args: array<string>) -> Result<Json, Error>
        redis_command(self.conn, args)
    end

    fun int_of(r: Result<Json, Error>) -> Result<int, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_number() then
            Result.Ok(j.as_number() as int)
        else
            Result.Err(Error { message: "Redis: a number was expected" })
        end
    end

    fun text_of(r: Result<Json, Error>) -> Result<string?, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_string() then
            Result<string?, Error>.Ok(j.as_string())
        else
            Result<string?, Error>.Ok(none)
        end
    end

    fun texts_of(r: Result<Json, Error>) -> Result<array<string>, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        let out: array<string> = []
        if j.is_array() then
            for x in j.as_array() do
                if x.is_string() then
                    out.push(x.as_string())
                end
            end
        end
        Result.Ok(out)
    end

    fun ok_of(r: Result<Json, Error>) -> Result<bool, Error>
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        Result.Ok(j.is_string() and j.as_string() == "OK")
    end

    --" The value of a key, or `none`. --"
    public fun get(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["GET", key]))
    end

    public fun set(self, key: string, value: string) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value]))
    end

    --" Sets a value that expires after `seconds`. --"
    public fun set_ex(self, key: string, value: string, seconds: int) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value, "EX", "{seconds}"]))
    end

    --" Sets the value only if the key does not exist (a lock): `true` when it was set. It expires
    after `seconds`. --"
    public fun set_if_absent(self, key: string, value: string, seconds: int) -> Result<bool, Error>
        Redis.ok_of(self.command(["SET", key, value, "NX", "EX", "{seconds}"]))
    end

    --" Deletes a key; how many existed (0 or 1). --"
    public fun del(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["DEL", key]))
    end

    public fun exists(self, key: string) -> Result<bool, Error>
        let n = Redis.int_of(self.command(["EXISTS", key]))
        if n.is_err() then
            return Result.Err(n.unwrap_err())
        end
        Result.Ok(n.unwrap() > 0)
    end

    --" Adds 1 to a number (0 if the key does not exist); gives the new value. --"
    public fun incr(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["INCR", key]))
    end

    --" The key expires after `seconds`; `false` if it does not exist. --"
    public fun expire(self, key: string, seconds: int) -> Result<bool, Error>
        let n = Redis.int_of(self.command(["EXPIRE", key, "{seconds}"]))
        if n.is_err() then
            return Result.Err(n.unwrap_err())
        end
        Result.Ok(n.unwrap() > 0)
    end

    --" Puts a value at the start of a list; gives the length of the list. --"
    public fun lpush(self, key: string, value: string) -> Result<int, Error>
        Redis.int_of(self.command(["LPUSH", key, value]))
    end

    --" Puts a value at the end of a list; gives the length of the list. --"
    public fun rpush(self, key: string, value: string) -> Result<int, Error>
        Redis.int_of(self.command(["RPUSH", key, value]))
    end

    public fun lpop(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["LPOP", key]))
    end

    public fun rpop(self, key: string) -> Result<string?, Error>
        Redis.text_of(self.command(["RPOP", key]))
    end

    public fun llen(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["LLEN", key]))
    end

    public fun lrange(self, key: string, start: int, stop: int) -> Result<array<string>, Error>
        Redis.texts_of(self.command(["LRANGE", key, "{start}", "{stop}"]))
    end

    --" Takes the first element of the first non-empty list, waiting up to `seconds` for one (0: for
    ever). Gives (list, element), or `none` when the time ran out. --"
    public fun blpop(self, keys: array<string>, seconds: int) -> Result<(string, string)?, Error>
        let words = ["BLPOP"]
        for k in keys do
            words.push(k)
        end
        words.push("{seconds}")
        let r = self.command(words)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if not j.is_array() then
            return Result.Ok(none)
        end
        let parts = j.as_array()
        if parts.len() != 2 then
            return Result.Ok(none)
        end
        Result<(string, string)?, Error>.Ok((parts[0].as_string(), parts[1].as_string()))
    end

    --" Adds a member to a sorted set with a score (or changes its score); how many were added. --"
    public fun zadd(self, key: string, score: float, member: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZADD", key, "{score}", member]))
    end

    public fun zrem(self, key: string, member: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZREM", key, member]))
    end

    public fun zcard(self, key: string) -> Result<int, Error>
        Redis.int_of(self.command(["ZCARD", key]))
    end

    --" The members whose score is from `min` to `max`, lowest first, at most `limit`. --"
    public fun zrange_by_score(self, key: string, min: float, max: float, limit: int) -> Result<array<string>, Error>
        Redis.texts_of(self.command(["ZRANGEBYSCORE", key, "{min}", "{max}", "LIMIT", "0", "{limit}"]))
    end

    --" Sends a message to a channel; how many subscribers received it. --"
    public fun publish(self, channel: string, message: string) -> Result<int, Error>
        Redis.int_of(self.command(["PUBLISH", channel, message]))
    end

    --" Runs a Lua script atomically: `KEYS` and `ARGV` are what you give. --"
    public fun eval(self, script: string, keys: array<string>, args: array<string>) -> Result<Json, Error>
        let words = ["EVAL", script, "{keys.len()}"]
        for k in keys do
            words.push(k)
        end
        for a in args do
            words.push(a)
        end
        self.command(words)
    end

    public fun close(self) -> Result<bool, Error>
        redis_close(self.conn)
    end
end

--" A connection that listens to channels (`Redis.subscriber`). --"
struct Subscription
    public conn: int
end

impl Subscription
    --" Listens to more channels. --"
    public fun subscribe(self, channels: array<string>) -> Result<bool, Error>
        self.send("SUBSCRIBE", channels)
    end

    public fun unsubscribe(self, channels: array<string>) -> Result<bool, Error>
        self.send("UNSUBSCRIBE", channels)
    end

    --" Listens to the channels that match patterns (`news.*`). --"
    public fun psubscribe(self, patterns: array<string>) -> Result<bool, Error>
        self.send("PSUBSCRIBE", patterns)
    end

    fun send(self, verb: string, names: array<string>) -> Result<bool, Error>
        let words = [verb]
        for n in names do
            words.push(n)
        end
        let r = redis_command(self.conn, words)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        Result.Ok(true)
    end

    --" The next message, waiting at most `timeout_ms` (0: only what already arrived). `none` when
    nothing came. --"
    public fun next(self, timeout_ms: int) -> Result<RedisMessage?, Error>
        let r = redis_poll(self.conn, timeout_ms)
        if r.is_err() then
            return Result.Err(r.unwrap_err())
        end
        let j = r.unwrap()
        if j.is_null() then
            return Result.Ok(none)
        end
        let channel = j.get("channel")
        let payload = j.get("payload")
        if channel == none or payload == none or not channel.is_string() or not payload.is_string() then
            return Result.Ok(none)
        end
        let pattern = j.get("pattern")
        let p: string? = if pattern != none and pattern.is_string() then pattern.as_string() else none end
        Result<RedisMessage?, Error>.Ok(RedisMessage { channel: channel.as_string(), payload: payload.as_string(), pattern: p })
    end

    public fun close(self) -> Result<bool, Error>
        redis_close(self.conn)
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
            Result.Ok(Db { conn: r.unwrap() })
        else
            Result.Err(r.unwrap_err())
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
    Alias(usize),
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
    /// A narrowed view, kept in a local scope, of a variable of the module.
    pub from_module: bool,
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
    /// The function the next `check_args` call is about (to record its type arguments).
    pub callee_hint: Option<FnId>,
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
            callee_hint: None,
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
        // an alias nobody uses is still checked
        for id in 0..self.defs.aliases.len() {
            let a = &self.defs.aliases[id];
            let (path, span) = (
                vec![crate::ast::Ident {
                    name: a.name.clone(),
                    span: a.span,
                }],
                a.span,
            );
            self.cur_mod = self.defs.aliases[id].module;
            self.item_to_type(ItemRef::Alias(id), &path, &[], span);
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
        self.finish_expr_tys();
    }
}
