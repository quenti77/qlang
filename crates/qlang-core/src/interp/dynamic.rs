//! Values whose type is looked at while the program runs: `Any`, `Json.of` and `Json.to<T>`,
//! and the `Reflect` calls that read, build or call things by name.

use super::*;

/// What a `Json` is called in `kind`.
const MAX_DEPTH: usize = 64;

/// A data problem found while reading a `Json` (`Err`), or a run-time failure (`Fail`).
type Read<T> = R<Result<T, String>>;

macro_rules! data {
    ($e:expr) => {
        match $e? {
            Ok(v) => v,
            Err(m) => return Ok(Err(m)),
        }
    };
}

impl Interp<'_> {
    pub(super) fn dynamic(&mut self, op: &ReflectOp, args: &[Value], span: Span) -> R<Value> {
        // a generic function's own type parameters are known now
        let op = &match op {
            ReflectOp::TypeName(t) => ReflectOp::TypeName(self.runtime_ty(t)),
            ReflectOp::AnyOf(t) => ReflectOp::AnyOf(self.runtime_ty(t)),
            ReflectOp::AnyAs(t) => ReflectOp::AnyAs(self.runtime_ty(t)),
            ReflectOp::Construct(t) => ReflectOp::Construct(self.runtime_ty(t)),
            ReflectOp::JsonTo(t) => ReflectOp::JsonTo(self.runtime_ty(t)),
            other => other.clone(),
        };
        Ok(match op {
            ReflectOp::Structs(m) => {
                let defs = &self.prog.defs;
                let ids: Vec<StructId> = (0..defs.structs.len())
                    .filter(|&s| {
                        let d = &defs.structs[s];
                        d.module == *m && d.tparams.is_empty() && !d.annotation
                    })
                    .collect();
                Value::array(ids.into_iter().map(|s| self.struct_info(s)).collect())
            }
            ReflectOp::TypeName(t) => Value::str(&t.show(&self.prog.defs)),
            ReflectOp::AnyOf(t) => match &args[0] {
                v @ Value::Any(_) => v.clone(),
                v => self.any_box(v.clone(), t),
            },
            ReflectOp::AnyAs(t) => match &args[0] {
                Value::Any(a) if self.any_matches(a, t) => a.value.clone(),
                _ => Value::None,
            },
            ReflectOp::AnyTypeName => match &args[0] {
                Value::Any(a) => Value::str(&a.ty.show(&self.prog.defs)),
                _ => Value::str(""),
            },
            ReflectOp::Callables(m) => {
                let fids: Vec<FnId> = self
                    .reflectable_functions(*m)
                    .filter(|&fid| {
                        self.prog.defs.fns[fid]
                            .decl
                            .as_ref()
                            .is_some_and(|d| d.tparams.is_empty())
                    })
                    .collect();
                Value::array(fids.into_iter().map(|f| self.callable_of(f)).collect())
            }
            ReflectOp::Callable(m) => {
                let Value::Str(name) = &args[0] else {
                    return Ok(Value::None);
                };
                let found = self.reflectable_functions(*m).find(|&fid| {
                    let def = &self.prog.defs.fns[fid];
                    def.name == **name && def.decl.as_ref().is_some_and(|d| d.tparams.is_empty())
                });
                match found {
                    None => Value::None,
                    Some(fid) => self.callable_of(fid),
                }
            }
            ReflectOp::Decode => {
                let Value::Any(holder) = &args[0] else {
                    return Ok(self.err_result("not a type"));
                };
                let ty = holder.ty.clone();
                match self.value_from_json(&args[1], &ty, "", 0, span)? {
                    Ok(v) => self.ok_result(self.any_box(v, &ty)),
                    Err(m) => self.err_result(&m),
                }
            }
            ReflectOp::ParseText => {
                let (Value::Any(holder), Value::Str(text)) = (&args[0], &args[1]) else {
                    return Ok(self.err_result("not a type"));
                };
                let ty = holder.ty.clone();
                match self.parse_scalar(text, &ty) {
                    Ok(v) => self.ok_result(self.any_box(v, &ty)),
                    Err(m) => self.err_result(&m),
                }
            }
            ReflectOp::Invoke => self.invoke(&args[0], &args[1], span)?,
            ReflectOp::Get => self.reflect_get(&args[0], &args[1]),
            ReflectOp::Construct(t) => self.construct(t, &args[0])?,
            ReflectOp::JsonOf => self.json_from_value(&args[0], 0, span)?,
            ReflectOp::JsonText => {
                let mut out = String::new();
                self.json_write(&args[0], &mut out, 0, span)?;
                Value::str(&out)
            }
            ReflectOp::JsonTo(t) => match self.value_from_json(&args[0], t, "", 0, span)? {
                Ok(v) => self.ok_result(v),
                Err(m) => self.err_result(&m),
            },
            // the other operations are handled in `reflect`
            _ => Value::None,
        })
    }

    /// A `Callable` for the function `fid`.
    fn callable_of(&self, fid: FnId) -> Value {
        let def = &self.prog.defs.fns[fid];
        let fty = Ty::Fn(def.params.clone(), Box::new(def.ret.clone()));
        let mut fields = self.function_fields(fid);
        fields.retain(|(k, _)| *k != "has_self");
        fields.push(("target", self.any_box(self.fn_value(fid), &fty)));
        self.info("Callable", &fields)
    }

    // ------------------------------------------------------------------ Any

    pub(super) fn any_box(&self, value: Value, ty: &Ty) -> Value {
        let ty = self.dynamic_ty(&value, ty);
        Value::Any(Rc::new(AnyVal { value, ty }))
    }

    /// The type to remember for `value`, boxed where its static type was `ty`: the real
    /// struct of a value seen through a parent or a trait, no `?` on a value that is there.
    fn dynamic_ty(&self, value: &Value, ty: &Ty) -> Ty {
        match (value, ty) {
            (Value::None, _) => ty.clone(),
            (_, Ty::Nullable(inner)) => self.dynamic_ty(value, inner),
            (Value::Struct(o), Ty::Struct(sid, _)) if o.def == *sid => ty.clone(),
            (Value::Struct(o), Ty::Struct(..) | Ty::Trait(..)) => Ty::Struct(o.def, Vec::new()),
            _ => ty.clone(),
        }
    }

    /// Whether `a` can be read back as a `target`.
    fn any_matches(&self, a: &AnyVal, target: &Ty) -> bool {
        if a.ty == *target {
            return true;
        }
        match (&a.value, target) {
            (Value::Struct(o), Ty::Struct(sid, _)) => self.prog.defs.is_subclass(o.def, *sid),
            (Value::Struct(_), Ty::Trait(..)) => self.value_matches_ty(&a.value, target),
            _ => false,
        }
    }

    /// The value inside `arg` if it fits a parameter of type `want`, else why not.
    fn any_fits(&self, arg: &Value, want: &Ty) -> Result<Value, String> {
        let Value::Any(a) = arg else {
            return Err("not an `Any`".to_string());
        };
        if *want == Ty::Any {
            return Ok(arg.clone());
        }
        if let Ty::Nullable(inner) = want {
            if matches!(a.value, Value::None) {
                return Ok(Value::None);
            }
            return self.any_fits(arg, inner);
        }
        if self.any_matches(a, want) {
            Ok(a.value.clone())
        } else {
            Err(format!(
                "expected {}, found {}",
                want.show(&self.prog.defs),
                a.ty.show(&self.prog.defs)
            ))
        }
    }

    fn invoke(&mut self, target: &Value, list: &Value, span: Span) -> R<Value> {
        let (Value::Any(t), Value::Array(items)) = (target, list) else {
            return Ok(self.err_result("not a function"));
        };
        let Ty::Fn(params, ret) = &t.ty else {
            return Ok(self.err_result("not a function"));
        };
        let items: Vec<Value> = items.borrow().clone();
        if items.len() != params.len() {
            return Ok(self.err_result(&format!(
                "expected {} argument(s), found {}",
                params.len(),
                items.len()
            )));
        }
        let mut argv = Vec::with_capacity(items.len());
        for (i, (item, want)) in items.iter().zip(params.iter()).enumerate() {
            match self.any_fits(item, want) {
                Ok(v) => argv.push(v),
                Err(why) => {
                    return Ok(self.err_result(&format!("argument {}: {why}", i + 1)));
                }
            }
        }
        let out = self.call_value(&t.value.clone(), argv, span)?;
        Ok(self.ok_result(self.any_box(out, ret)))
    }

    /// The type of a value seen without its declaration (a field of a generic struct).
    fn ty_of_value(&self, v: &Value) -> Ty {
        match v {
            Value::Int(_) => Ty::Int,
            Value::Float(_) => Ty::Float,
            Value::Bool(_) => Ty::Bool,
            Value::Str(_) => Ty::Str,
            Value::Uuid(_) => Ty::Uuid,
            Value::Ulid(_) => Ty::Ulid,
            Value::DateTime(_) => Ty::DateTime,
            Value::None => Ty::NoneT,
            Value::Unit => Ty::Unit,
            Value::Struct(o) => Ty::Struct(o.def, Vec::new()),
            Value::Enum(e, ..) => Ty::Enum(*e as usize, Vec::new()),
            Value::Array(a) => Ty::Array(Box::new(
                a.borrow()
                    .first()
                    .map_or(Ty::Error, |x| self.ty_of_value(x)),
            )),
            Value::Tuple(items) => Ty::Tuple(items.iter().map(|x| self.ty_of_value(x)).collect()),
            Value::Any(a) => a.ty.clone(),
            _ => Ty::Error,
        }
    }

    fn reflect_get(&self, target: &Value, name: &Value) -> Value {
        let target = match target {
            Value::Any(a) => &a.value,
            v => v,
        };
        let (Value::Struct(o), Value::Str(name)) = (target, name) else {
            return Value::None;
        };
        let Some(v) = o.fields.borrow().get(name).cloned() else {
            return Value::None;
        };
        let declared = self
            .prog
            .defs
            .ancestry(o.def, &[])
            .into_iter()
            .find_map(|(sid, _)| {
                self.prog.defs.structs[sid]
                    .fields
                    .iter()
                    .find(|f| f.name == **name && !f.is_static)
                    .map(|f| f.ty.clone())
            });
        let ty = match declared {
            Some(t) if !t.contains_param() => t,
            _ => self.ty_of_value(&v),
        };
        self.any_box(v, &ty)
    }

    /// The fields of `ty` (a struct) with their types, parents first.
    fn fields_of(&self, ty: &Ty) -> Vec<(StructId, usize, FieldDef, Ty)> {
        let Ty::Struct(sid, targs) = ty else {
            return Vec::new();
        };
        let defs = &self.prog.defs;
        let mut out = Vec::new();
        for (asid, aargs) in defs.ancestry(*sid, targs).into_iter().rev() {
            let sd = &defs.structs[asid];
            let map: HashMap<ParamId, Ty> = sd.tparams.iter().cloned().zip(aargs).collect();
            for (i, f) in sd.fields.iter().enumerate() {
                if !f.is_static {
                    out.push((asid, i, f.clone(), f.ty.subst(&map)));
                }
            }
        }
        out
    }

    /// The value a field gets when nothing is given for it: its default, or `none`.
    fn field_fallback(&mut self, sid: StructId, f: &FieldDef, ty: &Ty) -> R<Option<Value>> {
        if let Some(d) = &f.default {
            let scope = Scope::new(Some(self.envs[self.prog.defs.structs[sid].module].clone()));
            return Ok(Some(self.eval(d, &scope)?));
        }
        Ok(matches!(ty, Ty::Nullable(_)).then_some(Value::None))
    }

    fn construct(&mut self, ty: &Ty, map: &Value) -> R<Value> {
        let Value::Map(m) = map else {
            return Ok(self.err_result("expected a map of fields"));
        };
        let given: Vec<(Value, Value)> = m.borrow().entries.clone();
        let Ty::Struct(sid, _) = ty else {
            return Ok(self.err_result("not a struct"));
        };
        let fields = self.fields_of(ty);
        let shown = ty.show(&self.prog.defs);
        for (k, _) in &given {
            let Value::Str(k) = k else { continue };
            if !fields.iter().any(|(_, _, f, _)| f.name == **k) {
                return Ok(self.err_result(&format!("`{shown}` has no field `{k}`")));
            }
        }
        let mut out = SmallMap::default();
        for (asid, _, f, fty) in &fields {
            let found = given
                .iter()
                .find(|(k, _)| matches!(k, Value::Str(k) if **k == f.name));
            let v = match found {
                Some((_, a)) => match self.any_fits(a, fty) {
                    Ok(v) => v,
                    Err(why) => {
                        return Ok(self.err_result(&format!("field `{}`: {why}", f.name)));
                    }
                },
                None => match self.field_fallback(*asid, f, fty)? {
                    Some(v) => v,
                    None => {
                        return Ok(self.err_result(&format!("missing field `{}`", f.name)));
                    }
                },
            };
            out.insert(&f.name, v);
        }
        let obj = Value::Struct(Rc::new(StructObj {
            def: *sid,
            fields: RefCell::new(out),
        }));
        Ok(self.ok_result(obj))
    }

    // ----------------------------------------------------------------- Json

    fn json_node(&self, kind: &str, fields: [(&str, Value); 5]) -> Value {
        let [(_, s), (_, n), (_, b), (_, a), (_, o)] = fields;
        self.new_struct(
            self.prelude_struct("Json"),
            &[
                ("kind", Value::str(kind)),
                ("str", s),
                ("num", n),
                ("bool_", b),
                ("arr", a),
                ("obj", o),
            ],
        )
    }

    fn json_scalar(&self, kind: &str, s: Value, n: Value, b: Value) -> Value {
        self.json_node(
            kind,
            [
                ("str", s),
                ("num", n),
                ("bool_", b),
                ("arr", Value::None),
                ("obj", Value::None),
            ],
        )
    }

    fn json_null(&self) -> Value {
        self.json_scalar("null", Value::None, Value::None, Value::None)
    }

    fn json_text(&self, s: &str) -> Value {
        self.json_scalar("string", Value::str(s), Value::None, Value::None)
    }

    /// The annotation `name` of field `i` of `sid`, with its first argument if it is a string.
    fn field_annotation(&self, sid: StructId, i: usize, name: &str) -> Option<Option<String>> {
        let list = self.prog.res.annotations.get(&AnnKey::Field(sid, i))?;
        let a = list.iter().find(|a| a.name == name)?;
        Some(match a.args.first() {
            Some((_, Lit::Str(s))) => Some(s.clone()),
            _ => None,
        })
    }

    /// The name of a field in Json, or `None` if `#[JsonSkip]` leaves it out.
    fn json_field_name(&self, sid: StructId, i: usize, f: &FieldDef) -> Option<String> {
        if self.field_annotation(sid, i, "JsonSkip").is_some() {
            return None;
        }
        Some(
            self.field_annotation(sid, i, "JsonName")
                .flatten()
                .unwrap_or_else(|| f.name.clone()),
        )
    }

    /// A map key as the name of a Json object member.
    fn key_text(&self, k: &Value) -> String {
        match k {
            Value::Str(s) => s.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => fmt_float(*f),
            Value::Bool(b) => b.to_string(),
            Value::Uuid(u) => ids::format_uuid(u.get()),
            Value::Ulid(u) => ids::format_ulid(u.get()),
            Value::DateTime(t) => datetime::to_iso(*t),
            Value::Enum(e, vi, _) => self.prog.defs.enums[*e as usize].variants[*vi as usize]
                .name
                .clone(),
            _ => String::new(),
        }
    }

    /// `Json.of`: the public fields of structs, plain enums as their name, the rest as it reads.
    fn json_from_value(&mut self, v: &Value, depth: usize, span: Span) -> R<Value> {
        if depth > MAX_DEPTH {
            return self.fail(
                "R082",
                "this value is nested too deeply for `Json.of` (does it contain itself?)",
                span,
            );
        }
        self.tick(span)?;
        Ok(match v {
            Value::None | Value::Unit => self.json_null(),
            Value::Int(i) => {
                self.json_scalar("number", Value::None, Value::Float(*i as f64), Value::None)
            }
            Value::Float(f) => {
                self.json_scalar("number", Value::None, Value::Float(*f), Value::None)
            }
            Value::Bool(b) => self.json_scalar("bool", Value::None, Value::None, Value::Bool(*b)),
            Value::Str(s) => self.json_text(s),
            Value::Uuid(u) => self.json_text(&ids::format_uuid(u.get())),
            Value::Ulid(u) => self.json_text(&ids::format_ulid(u.get())),
            Value::DateTime(t) => self.json_text(&datetime::to_iso(*t)),
            Value::Any(a) => {
                let inner = a.value.clone();
                self.json_from_value(&inner, depth + 1, span)?
            }
            Value::Array(items) => {
                let items: Vec<Value> = items.borrow().clone();
                self.json_array(&items, depth, span)?
            }
            Value::Tuple(items) => {
                let items: Vec<Value> = items.to_vec();
                self.json_array(&items, depth, span)?
            }
            Value::Map(m) => {
                let entries: Vec<(Value, Value)> = m.borrow().entries.clone();
                let mut obj = MapObj::default();
                for (k, val) in &entries {
                    let j = self.json_from_value(val, depth + 1, span)?;
                    obj.insert(Value::str(&self.key_text(k)), j);
                }
                self.json_object(obj)
            }
            Value::Enum(e, vi, data) => {
                let def = &self.prog.defs.enums[*e as usize];
                let variant = def.variants[*vi as usize].clone();
                match data {
                    Some(values) if !variant.fields.is_empty() => {
                        let values: Vec<Value> = values.to_vec();
                        let mut obj = MapObj::default();
                        obj.insert(Value::str("type"), self.json_text(&variant.name));
                        for ((name, _), val) in variant.fields.iter().zip(values.iter()) {
                            let j = self.json_from_value(val, depth + 1, span)?;
                            obj.insert(Value::str(name), j);
                        }
                        self.json_object(obj)
                    }
                    _ => self.json_text(&variant.name),
                }
            }
            Value::Struct(o) => {
                if o.def == self.prelude_struct("Json") {
                    return Ok(v.clone());
                }
                let fields = self.fields_of(&Ty::Struct(o.def, Vec::new()));
                let mut obj = MapObj::default();
                for (asid, i, f, _) in &fields {
                    if f.vis != Visibility::Public {
                        continue;
                    }
                    let Some(name) = self.json_field_name(*asid, *i, f) else {
                        continue;
                    };
                    let fv = o.fields.borrow().get(&f.name).cloned();
                    let j = match fv {
                        Some(fv) => self.json_from_value(&fv, depth + 1, span)?,
                        None => self.json_null(),
                    };
                    obj.insert(Value::str(&name), j);
                }
                self.json_object(obj)
            }
            Value::Func(_) | Value::Range(..) | Value::RangeIncl(..) => {
                return self.fail(
                    "R084",
                    "a function or a range cannot be turned into Json",
                    span,
                );
            }
        })
    }

    fn json_array(&mut self, items: &[Value], depth: usize, span: Span) -> R<Value> {
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            out.push(self.json_from_value(it, depth + 1, span)?);
        }
        Ok(self.json_node(
            "array",
            [
                ("str", Value::None),
                ("num", Value::None),
                ("bool_", Value::None),
                ("arr", Value::array(out)),
                ("obj", Value::None),
            ],
        ))
    }

    fn json_object(&self, obj: MapObj) -> Value {
        self.json_node(
            "object",
            [
                ("str", Value::None),
                ("num", Value::None),
                ("bool_", Value::None),
                ("arr", Value::None),
                ("obj", Value::Map(Rc::new(RefCell::new(obj)))),
            ],
        )
    }

    /// The kind of a `Json` and its parts: (kind, str, num, bool, arr, obj).
    #[allow(clippy::type_complexity)]
    fn json_parts(
        &self,
        j: &Value,
    ) -> Option<(
        String,
        Option<Rc<str>>,
        Option<f64>,
        Option<bool>,
        Option<Vec<Value>>,
        Option<Vec<(Value, Value)>>,
    )> {
        let Value::Struct(o) = j else { return None };
        let f = o.fields.borrow();
        let kind = match f.get("kind") {
            Some(Value::Str(k)) => k.to_string(),
            _ => return None,
        };
        let s = match f.get("str") {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let n = match f.get("num") {
            Some(Value::Float(n)) => Some(*n),
            _ => None,
        };
        let b = match f.get("bool_") {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        };
        let a = match f.get("arr") {
            Some(Value::Array(a)) => Some(a.borrow().clone()),
            _ => None,
        };
        let m = match f.get("obj") {
            Some(Value::Map(m)) => Some(m.borrow().entries.clone()),
            _ => None,
        };
        Some((kind, s, n, b, a, m))
    }

    /// A text read as a scalar type, for the parts of a request that are only text.
    fn parse_scalar(&self, text: &str, ty: &Ty) -> Result<Value, String> {
        let shown = ty.show(&self.prog.defs);
        let bad = || format!("`{text}` is not a valid {shown}");
        match ty {
            Ty::Nullable(inner) => {
                if text.is_empty() {
                    Ok(Value::None)
                } else {
                    self.parse_scalar(text, inner)
                }
            }
            Ty::Str => Ok(Value::str(text)),
            Ty::Int => text.parse::<i64>().map(Value::Int).map_err(|_| bad()),
            Ty::Float => text
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(Value::Float)
                .ok_or_else(bad),
            Ty::Bool => match text {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(bad()),
            },
            Ty::Uuid => ids::parse_uuid(text)
                .map(|u| Value::Uuid(Id::new(u)))
                .ok_or_else(bad),
            Ty::Ulid => ids::parse_ulid(text)
                .map(|u| Value::Ulid(Id::new(u)))
                .ok_or_else(bad),
            Ty::DateTime => datetime::parse(text).map(Value::DateTime).ok_or_else(bad),
            _ => Err(format!("a {shown} cannot be read from text")),
        }
    }

    /// Writes a `Json` as JSON text.
    fn json_write(&mut self, j: &Value, out: &mut String, depth: usize, span: Span) -> R<()> {
        if depth > MAX_DEPTH {
            return self.fail("R082", "this Json is nested too deeply", span);
        }
        self.tick(span)?;
        let Some((kind, s, n, b, arr, obj)) = self.json_parts(j) else {
            out.push_str("null");
            return Ok(());
        };
        match kind.as_str() {
            "string" => json_quote(&s.unwrap_or_else(|| Rc::from("")), out),
            "number" => {
                let f = n.unwrap_or(0.0);
                if !f.is_finite() {
                    out.push_str("null");
                } else if f.fract() == 0.0 && f.abs() < 1e15 {
                    out.push_str(&format!("{}", f as i64));
                } else {
                    out.push_str(&format!("{f}"));
                }
            }
            "bool" => out.push_str(if b == Some(true) { "true" } else { "false" }),
            "array" => {
                out.push('[');
                for (i, it) in arr.unwrap_or_default().iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    self.json_write(it, out, depth + 1, span)?;
                }
                out.push(']');
            }
            "object" => {
                out.push('{');
                for (i, (k, v)) in obj.unwrap_or_default().iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    json_quote(&self.key_text(k), out);
                    out.push(':');
                    self.json_write(v, out, depth + 1, span)?;
                }
                out.push('}');
            }
            _ => out.push_str("null"),
        }
        Ok(())
    }

    /// `Json.to<T>`: reads `j` as a `ty`, or says where and why it cannot.
    fn value_from_json(
        &mut self,
        j: &Value,
        ty: &Ty,
        path: &str,
        depth: usize,
        span: Span,
    ) -> Read<Value> {
        if depth > MAX_DEPTH {
            return self.fail("R082", "this Json is nested too deeply", span);
        }
        self.tick(span)?;
        let shown = ty.show(&self.prog.defs);
        let at = |msg: String| -> String {
            if path.is_empty() {
                msg
            } else {
                format!("{path}: {msg}")
            }
        };
        let Some((kind, s, n, b, arr, obj)) = self.json_parts(j) else {
            return Ok(Err(at("not a Json value".to_string())));
        };
        let wrong = |expected: &str| Ok(Err(at(format!("expected {expected}, found {kind}"))));
        if let Ty::Nullable(inner) = ty {
            if kind == "null" {
                return Ok(Ok(Value::None));
            }
            return self.value_from_json(j, inner, path, depth + 1, span);
        }
        match ty {
            Ty::Int => match n {
                Some(f) if f.fract() == 0.0 && (-9.2e18..=9.2e18).contains(&f) => {
                    Ok(Ok(Value::Int(f as i64)))
                }
                Some(_) => Ok(Err(at(
                    "expected an int, found a number with decimals".to_string()
                ))),
                None => wrong("an int"),
            },
            Ty::Float => match n {
                Some(f) => Ok(Ok(Value::Float(f))),
                None => wrong("a number"),
            },
            Ty::Bool => match b {
                Some(x) => Ok(Ok(Value::Bool(x))),
                None => wrong("a bool"),
            },
            Ty::Str => match s {
                Some(x) if kind == "string" => Ok(Ok(Value::Str(x))),
                _ => wrong("a string"),
            },
            Ty::Uuid | Ty::Ulid | Ty::DateTime => {
                let Some(text) = s.filter(|_| kind == "string") else {
                    return wrong(&format!("a {shown} as a string"));
                };
                let v = match ty {
                    Ty::Uuid => ids::parse_uuid(&text).map(|u| Value::Uuid(Id::new(u))),
                    Ty::Ulid => ids::parse_ulid(&text).map(|u| Value::Ulid(Id::new(u))),
                    _ => datetime::parse(&text).map(Value::DateTime),
                };
                Ok(v.ok_or_else(|| at(format!("`{text}` is not a valid {shown}"))))
            }
            Ty::Array(t) => {
                let Some(items) = arr.filter(|_| kind == "array") else {
                    return wrong("an array");
                };
                let mut out = Vec::with_capacity(items.len());
                for (i, it) in items.iter().enumerate() {
                    out.push(data!(self.value_from_json(
                        it,
                        t,
                        &format!("{path}[{i}]"),
                        depth + 1,
                        span
                    )));
                }
                Ok(Ok(Value::array(out)))
            }
            Ty::Tuple(ts) => {
                let Some(items) = arr.filter(|_| kind == "array") else {
                    return wrong("an array");
                };
                if items.len() != ts.len() {
                    return Ok(Err(at(format!(
                        "expected {} values, found {}",
                        ts.len(),
                        items.len()
                    ))));
                }
                let mut out = Vec::with_capacity(items.len());
                for (i, (it, t)) in items.iter().zip(ts).enumerate() {
                    out.push(data!(self.value_from_json(
                        it,
                        t,
                        &format!("{path}[{i}]"),
                        depth + 1,
                        span
                    )));
                }
                Ok(Ok(Value::Tuple(Rc::from(out))))
            }
            Ty::Map(kt, vt) => {
                let Some(entries) = obj.filter(|_| kind == "object") else {
                    return wrong("an object");
                };
                let mut m = MapObj::default();
                for (k, val) in &entries {
                    let name = self.key_text(k);
                    let sub = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}.{name}")
                    };
                    let key = match &**kt {
                        Ty::Str => Value::str(&name),
                        Ty::Int => match name.parse::<i64>() {
                            Ok(i) => Value::Int(i),
                            Err(_) => return Ok(Err(at(format!("`{name}` is not an int key")))),
                        },
                        _ => {
                            let kj = self.json_text(&name);
                            data!(self.value_from_json(&kj, kt, &sub, depth + 1, span))
                        }
                    };
                    let v = data!(self.value_from_json(val, vt, &sub, depth + 1, span));
                    m.insert(key, v);
                }
                Ok(Ok(Value::Map(Rc::new(RefCell::new(m)))))
            }
            Ty::Struct(sid, _) if *sid == self.prelude_struct("Json") => Ok(Ok(j.clone())),
            Ty::Struct(sid, _) => {
                let Some(entries) = obj.filter(|_| kind == "object") else {
                    return wrong(&format!("an object for {shown}"));
                };
                let mut out = SmallMap::default();
                for (asid, i, f, fty) in self.fields_of(ty) {
                    let json_name = self.json_field_name(asid, i, &f);
                    let found = if f.vis == Visibility::Public {
                        json_name.as_ref().and_then(|n| {
                            entries
                                .iter()
                                .find(|(k, _)| matches!(k, Value::Str(k) if **k == **n))
                        })
                    } else {
                        None
                    };
                    let sub = if path.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{path}.{}", f.name)
                    };
                    let v = match found {
                        Some((_, fj)) => {
                            data!(self.value_from_json(fj, &fty, &sub, depth + 1, span))
                        }
                        None => match self.field_fallback(asid, &f, &fty)? {
                            Some(v) => v,
                            None if f.vis == Visibility::Public => {
                                return Ok(Err(format!("{sub}: missing")));
                            }
                            None => {
                                return Ok(Err(format!(
                                    "{sub}: a private field without a default cannot be read from Json"
                                )));
                            }
                        },
                    };
                    out.insert(&f.name, v);
                }
                Ok(Ok(Value::Struct(Rc::new(StructObj {
                    def: *sid,
                    fields: RefCell::new(out),
                }))))
            }
            Ty::Enum(eid, targs) => {
                let def = self.prog.defs.enums[*eid].clone();
                let map: HashMap<ParamId, Ty> = def
                    .tparams
                    .iter()
                    .cloned()
                    .zip(targs.iter().cloned())
                    .collect();
                let name = match (&s, &obj) {
                    (Some(n), _) if kind == "string" => n.to_string(),
                    (_, Some(entries)) if kind == "object" => {
                        match entries
                            .iter()
                            .find(|(k, _)| matches!(k, Value::Str(k) if &**k == "type"))
                        {
                            Some((_, Value::Struct(t))) => match t.fields.borrow().get("str") {
                                Some(Value::Str(n)) => n.to_string(),
                                _ => return Ok(Err(at("`type` must be a string".to_string()))),
                            },
                            _ => return Ok(Err(at(format!("{shown} needs a `type` member")))),
                        }
                    }
                    _ => return wrong(&format!("{shown} as a string or an object")),
                };
                let Some(vi) = def.variant_index(&name) else {
                    let names: Vec<&str> = def.variants.iter().map(|v| v.name.as_str()).collect();
                    return Ok(Err(at(format!(
                        "`{name}` is not a {shown} (expected {})",
                        names.join(", ")
                    ))));
                };
                let variant = &def.variants[vi];
                if variant.fields.is_empty() {
                    return Ok(Ok(Value::Enum(*eid as u32, vi as u32, None)));
                }
                let Some(entries) = obj else {
                    return Ok(Err(at(format!(
                        "`{name}` carries data: expected an object"
                    ))));
                };
                let mut values = Vec::new();
                for (fname, fty) in &variant.fields {
                    let fty = fty.subst(&map);
                    let sub = if path.is_empty() {
                        fname.clone()
                    } else {
                        format!("{path}.{fname}")
                    };
                    let found = entries
                        .iter()
                        .find(|(k, _)| matches!(k, Value::Str(k) if **k == **fname));
                    match found {
                        Some((_, fj)) => values.push(data!(self.value_from_json(
                            fj,
                            &fty,
                            &sub,
                            depth + 1,
                            span
                        ))),
                        None if matches!(fty, Ty::Nullable(_)) => values.push(Value::None),
                        None => return Ok(Err(format!("{sub}: missing"))),
                    }
                }
                Ok(Ok(Value::Enum(
                    *eid as u32,
                    vi as u32,
                    Some(Rc::new(values)),
                )))
            }
            _ => Ok(Err(at(format!("a Json cannot be read as {shown}")))),
        }
    }
}

/// `text` between quotes, with what JSON needs escaped.
fn json_quote(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
