//! `Reflect`: describes structs, enums and the functions of a file as values of the prelude's
//! `StructInfo`, `EnumInfo`, `FunctionInfo`..., from what the checker recorded.

use super::*;

impl Interp<'_> {
    pub(super) fn reflect(&mut self, op: &ReflectOp, args: &[Value]) -> Value {
        match op {
            ReflectOp::Struct(sid) => self.struct_info(*sid),
            ReflectOp::Enum(eid) => self.enum_info(*eid),
            ReflectOp::Functions(m) => {
                let infos = self
                    .reflectable_functions(*m)
                    .map(|fid| self.function_info(fid))
                    .collect();
                Value::array(infos)
            }
            ReflectOp::Find(m, ty) => {
                let (Ty::Fn(params, ret), Some(Value::Str(name))) = (ty, args.first()) else {
                    return Value::None;
                };
                let found = self.reflectable_functions(*m).find(|&fid| {
                    let def = &self.prog.defs.fns[fid];
                    def.name == **name
                        && def.params == *params
                        && def.ret == **ret
                        && def.decl.as_ref().is_some_and(|d| d.tparams.is_empty())
                });
                found.map_or(Value::None, |fid| self.fn_value(fid))
            }
        }
    }

    /// The functions declared at the top of module `m` (not methods, not `extern`).
    fn reflectable_functions(&self, m: ModId) -> impl Iterator<Item = FnId> + '_ {
        self.prog
            .defs
            .fns
            .iter()
            .enumerate()
            .filter(move |(_, f)| {
                f.module == m
                    && f.impl_id.is_none()
                    && !f.has_self
                    && f.trait_default_self.is_none()
                    && f.decl
                        .as_ref()
                        .is_some_and(|d| d.body.is_some() && !d.external)
            })
            .map(|(fid, _)| fid)
    }

    fn info(&self, ty: &str, fields: &[(&str, Value)]) -> Value {
        self.new_struct(self.prelude_struct(ty), fields)
    }

    fn type_name(&self, ty: &Ty) -> Value {
        Value::str(&ty.show(&self.prog.defs))
    }

    fn annotation_infos(&self, key: AnnKey) -> Value {
        let list = self.prog.res.annotations.get(&key);
        let infos = list
            .into_iter()
            .flatten()
            .map(|a| {
                let mut args = MapObj::default();
                for (name, lit) in &a.args {
                    args.insert(Value::str(name), self.json_of(lit));
                }
                self.info(
                    "AnnotationInfo",
                    &[
                        ("name", Value::str(&a.name)),
                        ("args", Value::Map(Rc::new(RefCell::new(args)))),
                    ],
                )
            })
            .collect();
        Value::array(infos)
    }

    /// An annotation argument as a `Json` value.
    fn json_of(&self, lit: &Lit) -> Value {
        let n = Value::None;
        let (kind, str, num, bool_, arr) = match lit {
            Lit::Int(i) => (
                "number",
                n.clone(),
                Value::Float(*i as f64),
                n.clone(),
                n.clone(),
            ),
            Lit::Float(f) => ("number", n.clone(), Value::Float(*f), n.clone(), n.clone()),
            Lit::Bool(b) => ("bool", n.clone(), n.clone(), Value::Bool(*b), n.clone()),
            Lit::Str(s) => ("string", Value::str(s), n.clone(), n.clone(), n.clone()),
            Lit::None => ("null", n.clone(), n.clone(), n.clone(), n.clone()),
            Lit::Array(items) => (
                "array",
                n.clone(),
                n.clone(),
                n.clone(),
                Value::array(items.iter().map(|i| self.json_of(i)).collect()),
            ),
        };
        self.info(
            "Json",
            &[
                ("kind", Value::str(kind)),
                ("str", str),
                ("num", num),
                ("bool_", bool_),
                ("arr", arr),
                ("obj", n),
            ],
        )
    }

    fn param_info(&self, name: &str, ty: &Ty, annotations: Value) -> Value {
        self.info(
            "ParamInfo",
            &[
                ("name", Value::str(name)),
                ("type_name", self.type_name(ty)),
                ("annotations", annotations),
            ],
        )
    }

    fn struct_info(&self, sid: StructId) -> Value {
        let def = &self.prog.defs.structs[sid];
        let fields = def
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                self.info(
                    "FieldInfo",
                    &[
                        ("name", Value::str(&f.name)),
                        ("type_name", self.type_name(&f.ty)),
                        ("annotations", self.annotation_infos(AnnKey::Field(sid, i))),
                    ],
                )
            })
            .collect();
        self.info(
            "StructInfo",
            &[
                ("name", Value::str(&def.name)),
                ("fields", Value::array(fields)),
                ("annotations", self.annotation_infos(AnnKey::Struct(sid))),
            ],
        )
    }

    fn enum_info(&self, eid: EnumId) -> Value {
        let def = &self.prog.defs.enums[eid];
        let variants = def
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let fields = v
                    .fields
                    .iter()
                    .map(|(n, t)| self.param_info(n, t, Value::array(Vec::new())))
                    .collect();
                self.info(
                    "VariantInfo",
                    &[
                        ("name", Value::str(&v.name)),
                        ("fields", Value::array(fields)),
                        (
                            "annotations",
                            self.annotation_infos(AnnKey::Variant(eid, i)),
                        ),
                    ],
                )
            })
            .collect();
        self.info(
            "EnumInfo",
            &[
                ("name", Value::str(&def.name)),
                ("variants", Value::array(variants)),
                ("annotations", self.annotation_infos(AnnKey::Enum(eid))),
            ],
        )
    }

    fn function_info(&self, fid: FnId) -> Value {
        let def = &self.prog.defs.fns[fid];
        let names: Vec<&str> = def
            .decl
            .as_ref()
            .map(|d| d.params.iter().map(|p| p.name.name.as_str()).collect())
            .unwrap_or_default();
        let params = def
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                self.param_info(
                    names.get(i).copied().unwrap_or("_"),
                    t,
                    self.annotation_infos(AnnKey::Param(fid, i)),
                )
            })
            .collect();
        self.info(
            "FunctionInfo",
            &[
                ("name", Value::str(&def.name)),
                ("params", Value::array(params)),
                ("return_type", self.type_name(&def.ret)),
                ("annotations", self.annotation_infos(AnnKey::Fn(fid))),
            ],
        )
    }
}
