//! `extern fun`: calls into the host, with plain data copied in both directions.
//!
//! What may cross is decided by the checker (`boundary_problem`): `int`, `float`,
//! `bool`, `string`, enums, `T?`, tuples (an array), arrays, maps, `Json` and plain structs. A struct is a
//! `HostValue::Map` keyed by field names, an enum is the name of its variant.

use super::*;
use crate::host::HostValue;

/// Values nested deeper than this are refused (a shared array can contain itself).
const MAX_NESTING: usize = 100;

impl Interp<'_> {
    pub(super) fn call_extern(
        &mut self,
        decl: &Rc<FunDecl>,
        args: Vec<Value>,
        span: Span,
    ) -> R<Value> {
        let prog = self.prog;
        let fid = prog
            .defs
            .fns
            .iter()
            .position(|f| f.decl.as_ref().is_some_and(|d| Rc::ptr_eq(d, decl)))
            .expect("extern function without definition");
        let def = &prog.defs.fns[fid];
        let name = def.name.clone();

        let mut work = 0;
        let mut host_args = Vec::with_capacity(args.len());
        for (a, ty) in args.iter().zip(&def.params) {
            host_args.push(self.value_to_host(a, ty, 0, &mut work, span)?);
        }
        self.charge(work, span)?;

        // `Result<T, Error>`: a host failure is the caller's to handle
        let payload = prog_result_payload(prog, &def.ret);
        let answer = self.host.call(&name, host_args);
        match (answer, payload) {
            (Ok(h), Some(t)) => {
                let v = self.host_to_value(&h, t, 0, &mut work, span)?;
                self.charge(work, span)?;
                Ok(self.ok_result(v))
            }
            (Ok(h), None) => {
                let v = self.host_to_value(&h, &def.ret, 0, &mut work, span)?;
                self.charge(work, span)?;
                Ok(v)
            }
            (Err(e), Some(_)) => Ok(self.host_err_result(e)),
            (Err(HostError::Unsupported(_)), None) => self.fail(
                "R080",
                format!("`{name}` is not available in this environment"),
                span,
            ),
            (Err(HostError::Other(m)), None) => {
                self.fail("R081", format!("`{name}` failed: {m}"), span)
            }
        }
    }

    fn value_to_host(
        &self,
        v: &Value,
        ty: &Ty,
        depth: usize,
        work: &mut usize,
        span: Span,
    ) -> R<HostValue> {
        if depth > MAX_NESTING {
            return self.fail(
                "R082",
                "this value is nested too deeply to leave the program (does it contain itself?)",
                span,
            );
        }
        *work += 1;
        Ok(match (v, ty) {
            (Value::None | Value::Unit, _) => HostValue::Null,
            (Value::Int(i), _) => HostValue::Int(*i),
            (Value::Float(f), _) => HostValue::Float(*f),
            (Value::Bool(b), _) => HostValue::Bool(*b),
            (Value::Str(s), _) => HostValue::Str(s.to_string()),
            (Value::Bytes(b), _) => HostValue::Bytes(b.to_vec()),
            (Value::Enum(e, i, _), _) => HostValue::Str(
                self.prog.defs.enums[*e as usize].variants[*i as usize]
                    .name
                    .clone(),
            ),
            (Value::Array(items), _) => {
                let elem = match ty {
                    Ty::Nullable(t) => match &**t {
                        Ty::Array(e) => e,
                        _ => ty,
                    },
                    Ty::Array(e) => e,
                    _ => ty,
                };
                let mut out = Vec::new();
                for item in items.borrow().iter() {
                    out.push(self.value_to_host(item, elem, depth + 1, work, span)?);
                }
                HostValue::Array(out)
            }
            (Value::Tuple(items), _) => {
                let tys: &[Ty] = match ty {
                    Ty::Tuple(ts) => ts,
                    Ty::Nullable(t) => match &**t {
                        Ty::Tuple(ts) => ts,
                        _ => &[],
                    },
                    _ => &[],
                };
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let t = tys.get(i).unwrap_or(&Ty::Error);
                    out.push(self.value_to_host(item, t, depth + 1, work, span)?);
                }
                HostValue::Array(out)
            }
            (Value::Map(m), _) => {
                let (kt, vt) = match ty {
                    Ty::Map(k, v) => (&**k, &**v),
                    Ty::Nullable(t) => match &**t {
                        Ty::Map(k, v) => (&**k, &**v),
                        _ => (ty, ty),
                    },
                    _ => (ty, ty),
                };
                let mut out = Vec::new();
                for (k, val) in m.borrow().entries.iter() {
                    out.push((
                        self.value_to_host(k, kt, depth + 1, work, span)?,
                        self.value_to_host(val, vt, depth + 1, work, span)?,
                    ));
                }
                HostValue::Map(out)
            }
            (Value::Struct(o), _) => {
                if o.def == self.prelude_struct("Json") {
                    return self.json_to_host(v, depth, work, span);
                }
                let def = &self.prog.defs.structs[o.def];
                let fields = o.fields.borrow();
                let mut out = Vec::new();
                for f in def.fields.iter().filter(|f| !f.is_static) {
                    let fv = fields.get(&f.name).cloned().unwrap_or(Value::None);
                    out.push((
                        HostValue::Str(f.name.clone()),
                        self.value_to_host(&fv, &f.ty, depth + 1, work, span)?,
                    ));
                }
                HostValue::Map(out)
            }
            _ => HostValue::Null,
        })
    }

    fn json_to_host(&self, v: &Value, depth: usize, work: &mut usize, span: Span) -> R<HostValue> {
        if depth > MAX_NESTING {
            return self.fail(
                "R082",
                "this value is nested too deeply to leave the program (does it contain itself?)",
                span,
            );
        }
        *work += 1;
        let Value::Struct(o) = v else {
            return Ok(HostValue::Null);
        };
        let fields = o.fields.borrow();
        let kind = match fields.get("kind") {
            Some(Value::Str(k)) => k.to_string(),
            _ => return Ok(HostValue::Null),
        };
        Ok(
            match (
                kind.as_str(),
                fields.get("str"),
                fields.get("num"),
                fields.get("bool_"),
            ) {
                ("string", Some(Value::Str(s)), _, _) => HostValue::Str(s.to_string()),
                ("number", _, Some(Value::Float(n)), _) => HostValue::Float(*n),
                ("bool", _, _, Some(Value::Bool(b))) => HostValue::Bool(*b),
                ("array", ..) => match fields.get("arr") {
                    Some(Value::Array(items)) => {
                        let mut out = Vec::new();
                        for item in items.borrow().iter() {
                            out.push(self.json_to_host(item, depth + 1, work, span)?);
                        }
                        HostValue::Array(out)
                    }
                    _ => HostValue::Null,
                },
                ("object", ..) => match fields.get("obj") {
                    Some(Value::Map(m)) => {
                        let mut out = Vec::new();
                        for (k, val) in m.borrow().entries.iter() {
                            out.push((
                                self.json_to_host(&self.json_string(k), depth + 1, work, span)?,
                                self.json_to_host(val, depth + 1, work, span)?,
                            ));
                        }
                        HostValue::Map(out)
                    }
                    _ => HostValue::Null,
                },
                _ => HostValue::Null,
            },
        )
    }

    /// A map key (always a string for a `map<string, Json>`) as a `Json` string.
    fn json_string(&self, k: &Value) -> Value {
        self.json_value(
            "string",
            Value::str(&k_text(k)),
            Value::None,
            Value::None,
            Value::None,
            Value::None,
        )
    }

    fn json_value(
        &self,
        kind: &str,
        s: Value,
        n: Value,
        b: Value,
        arr: Value,
        obj: Value,
    ) -> Value {
        self.new_struct(
            self.prelude_struct("Json"),
            &[
                ("kind", Value::str(kind)),
                ("str", s),
                ("num", n),
                ("bool_", b),
                ("arr", arr),
                ("obj", obj),
            ],
        )
    }

    fn host_to_json(&self, h: &HostValue, depth: usize, work: &mut usize, span: Span) -> R<Value> {
        if depth > MAX_NESTING {
            return self.fail("R082", "the host returned a value nested too deeply", span);
        }
        *work += 1;
        let none = || Value::None;
        Ok(match h {
            HostValue::Null => self.json_value("null", none(), none(), none(), none(), none()),
            HostValue::Bool(b) => {
                self.json_value("bool", none(), none(), Value::Bool(*b), none(), none())
            }
            // a `Json` has no bytes: they read as base64 text
            HostValue::Bytes(b) => self.json_value(
                "string",
                Value::str(&crate::codec::base64_encode(b, false)),
                none(),
                none(),
                none(),
                none(),
            ),
            HostValue::Int(i) => self.json_value(
                "number",
                none(),
                Value::Float(*i as f64),
                none(),
                none(),
                none(),
            ),
            HostValue::Float(f) => {
                self.json_value("number", none(), Value::Float(*f), none(), none(), none())
            }
            HostValue::Str(s) => {
                self.json_value("string", Value::str(s), none(), none(), none(), none())
            }
            HostValue::Array(items) => {
                self.check_size(items.len(), span)?;
                let mut out = Vec::new();
                for item in items {
                    out.push(self.host_to_json(item, depth + 1, work, span)?);
                }
                self.json_value(
                    "array",
                    none(),
                    none(),
                    none(),
                    Value::Array(Rc::new(RefCell::new(out))),
                    none(),
                )
            }
            HostValue::Map(entries) => {
                self.check_size(entries.len(), span)?;
                let mut m = MapObj::default();
                for (k, val) in entries {
                    let HostValue::Str(key) = k else {
                        return self.fail(
                            "R083",
                            "the host returned a `Json` object with a key that is not a string",
                            span,
                        );
                    };
                    m.insert(
                        Value::str(key),
                        self.host_to_json(val, depth + 1, work, span)?,
                    );
                }
                self.json_value(
                    "object",
                    none(),
                    none(),
                    none(),
                    none(),
                    Value::Map(Rc::new(RefCell::new(m))),
                )
            }
        })
    }

    fn check_size(&self, len: usize, span: Span) -> R<()> {
        if len > self.limits.max_alloc {
            return self.fail(
                "R084",
                "the host returned a collection that is too big",
                span,
            );
        }
        Ok(())
    }

    fn host_to_value(
        &self,
        h: &HostValue,
        ty: &Ty,
        depth: usize,
        work: &mut usize,
        span: Span,
    ) -> R<Value> {
        if depth > MAX_NESTING {
            return self.fail("R082", "the host returned a value nested too deeply", span);
        }
        *work += 1;
        let bad = |what: &str| -> R<Value> {
            self.fail(
                "R083",
                format!(
                    "the host returned {} where {what} was expected",
                    host_kind(h)
                ),
                span,
            )
        };
        match (ty, h) {
            (Ty::Error, _) | (Ty::Unit, HostValue::Null) => Ok(Value::Unit),
            (Ty::Nullable(_), HostValue::Null) => Ok(Value::None),
            (Ty::Nullable(t), _) => self.host_to_value(h, t, depth, work, span),
            (Ty::Int, HostValue::Int(i)) => Ok(Value::Int(*i)),
            (Ty::Float, HostValue::Float(f)) => Ok(Value::Float(*f)),
            (Ty::Float, HostValue::Int(i)) => Ok(Value::Float(*i as f64)),
            (Ty::Bool, HostValue::Bool(b)) => Ok(Value::Bool(*b)),
            (Ty::Str, HostValue::Str(s)) => {
                self.check_size(s.len(), span)?;
                Ok(Value::str(s))
            }
            (Ty::Bytes, HostValue::Bytes(b)) => {
                self.check_size(b.len(), span)?;
                Ok(Value::Bytes(Rc::from(b.clone())))
            }
            // an external command sends bytes as base64 text
            (Ty::Bytes, HostValue::Str(s)) => match crate::codec::base64_decode(s) {
                Some(b) => Ok(Value::Bytes(Rc::from(b))),
                None => bad("base64 text"),
            },
            (Ty::Enum(e, _), HostValue::Str(name)) => {
                match self.prog.defs.enums[*e].variant_index(name) {
                    Some(i) => Ok(Value::Enum(*e as u32, i as u32, None)),
                    None => self.fail(
                        "R083",
                        format!(
                            "the host returned `{name}`, which is not a variant of `{}`",
                            self.prog.defs.enums[*e].name
                        ),
                        span,
                    ),
                }
            }
            (Ty::Array(t), HostValue::Array(items)) => {
                self.check_size(items.len(), span)?;
                let mut out = Vec::new();
                for item in items {
                    out.push(self.host_to_value(item, t, depth + 1, work, span)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(out))))
            }
            (Ty::Tuple(ts), HostValue::Array(items)) if ts.len() == items.len() => {
                let mut out = Vec::new();
                for (item, t) in items.iter().zip(ts) {
                    out.push(self.host_to_value(item, t, depth + 1, work, span)?);
                }
                Ok(Value::Tuple(Rc::from(out)))
            }
            (Ty::Map(kt, vt), HostValue::Map(entries)) => {
                self.check_size(entries.len(), span)?;
                let mut m = MapObj::default();
                for (k, val) in entries {
                    // a JSON object only has text keys: `{"1": ...}` is a valid `map<int, V>`
                    let key = match (&**kt, k) {
                        (Ty::Int, HostValue::Str(s)) if s.parse::<i64>().is_ok() => {
                            Value::Int(s.parse().unwrap())
                        }
                        (Ty::Bool, HostValue::Str(s)) if s == "true" || s == "false" => {
                            Value::Bool(s == "true")
                        }
                        _ => self.host_to_value(k, kt, depth + 1, work, span)?,
                    };
                    let val = self.host_to_value(val, vt, depth + 1, work, span)?;
                    if !m.insert(key, val) {
                        return bad("a valid map key");
                    }
                }
                Ok(Value::Map(Rc::new(RefCell::new(m))))
            }
            (Ty::Struct(sid, _), _) if *sid == self.prelude_struct("Json") => {
                self.host_to_json(h, depth, work, span)
            }
            (Ty::Struct(sid, _), HostValue::Map(entries)) => {
                let def = &self.prog.defs.structs[*sid];
                let mut fields = crate::smallmap::SmallMap::default();
                for f in def.fields.iter().filter(|f| !f.is_static) {
                    let found = entries
                        .iter()
                        .find(|(k, _)| matches!(k, HostValue::Str(n) if *n == f.name));
                    let v = match found {
                        Some((_, hv)) => self.host_to_value(hv, &f.ty, depth + 1, work, span)?,
                        None if matches!(f.ty, Ty::Nullable(_)) => Value::None,
                        // what the host does not know about (the `cookies` of a `Response`)
                        None if matches!(f.ty, Ty::Array(_)) => Value::array(Vec::new()),
                        None => {
                            return self.fail(
                                "R083",
                                format!("the host's `{}` has no field `{}`", def.name, f.name),
                                span,
                            );
                        }
                    };
                    fields.insert(&f.name, v);
                }
                Ok(Value::Struct(Rc::new(StructObj {
                    def: *sid,
                    fields: RefCell::new(fields),
                })))
            }
            (t, _) => bad(&format!("`{}`", ty_word(t))),
        }
    }
}

fn host_kind(h: &HostValue) -> &'static str {
    match h {
        HostValue::Null => "nothing (null)",
        HostValue::Bool(_) => "a bool",
        HostValue::Int(_) => "an int",
        HostValue::Float(_) => "a float",
        HostValue::Str(_) => "a string",
        HostValue::Bytes(_) => "bytes",
        HostValue::Array(_) => "an array",
        HostValue::Map(_) => "a map",
    }
}

fn ty_word(t: &Ty) -> &'static str {
    match t {
        Ty::Int => "int",
        Ty::Float => "float",
        Ty::Bool => "bool",
        Ty::Str => "string",
        Ty::Bytes => "bytes",
        Ty::Array(_) => "array",
        Ty::Map(..) => "map",
        Ty::Enum(..) => "enum",
        Ty::Struct(..) => "struct",
        _ => "another type",
    }
}

fn k_text(k: &Value) -> String {
    match k {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    }
}

/// The `T` of a `Result<T, Error>` return type.
fn prog_result_payload<'a>(prog: &crate::check::Program, ret: &'a Ty) -> Option<&'a Ty> {
    let Ty::Enum(eid, args) = ret else {
        return None;
    };
    let s = &prog.defs.enums[*eid];
    (s.module == crate::check::PRELUDE_MOD && s.name == "Result" && args.len() == 2)
        .then(|| &args[0])
}
