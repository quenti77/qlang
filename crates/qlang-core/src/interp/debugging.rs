//! The interpreter's side of `qlang dap` (see `crate::debug`).

use super::*;
use crate::debug::{Action, Frame, Scope as DbgScope, Snapshot, Var};

/// How deep a snapshot looks into values, and how many members of one value it lists.
const MAX_DEPTH: usize = 4;
const MAX_MEMBERS: usize = 200;

impl Interp<'_> {
    /// Called before every statement when a debugger is attached.
    pub(super) fn debug_statement(&mut self, span: Span, env: &Env) -> R<()> {
        if let Some(top) = self.dbg_stack.last_mut() {
            *top = (span, Some(env.clone()));
        }
        let Some(mut hook) = self.dbg.take() else {
            return Ok(());
        };
        let loc = self.prog.sources.locate(span);
        let depth = self.frames.len();
        let action = hook.check(&loc.file, loc.line, depth);
        let mut go = action != Action::Abort;
        if action == Action::Stop {
            let snapshot = self.snapshot();
            go = hook.stopped(snapshot);
        }
        self.dbg = Some(hook);
        if go {
            Ok(())
        } else {
            self.fail("R907", "the debugging session ended", span)
        }
    }

    /// The calls in progress and their variables.
    fn snapshot(&self) -> Snapshot {
        let mut snap = Snapshot::default();
        // frame 0 is the program itself; frame i is the call frames[i - 1]
        for i in (0..self.dbg_stack.len()).rev() {
            let (span, env) = &self.dbg_stack[i];
            let loc = self.prog.sources.locate(*span);
            let name = if i == 0 {
                "(program)".to_string()
            } else {
                self.frames[i - 1].0.as_str().to_string()
            };
            let mut scopes = Vec::new();
            if let Some(env) = env {
                let mut locals: Vec<Var> = Vec::new();
                let mut globals: Vec<Var> = Vec::new();
                let mut cur = Some(env.clone());
                let mut in_globals = false;
                while let Some(scope) = cur {
                    if self.envs.iter().any(|m| Rc::ptr_eq(m, &scope)) {
                        in_globals = true;
                    }
                    let target = if in_globals && i > 0 {
                        &mut globals
                    } else {
                        &mut locals
                    };
                    for (n, v) in scope.entries() {
                        if n.starts_with(|c: char| c.is_alphabetic() || c == '_') {
                            target.push(self.var(&mut snap, &n, &v, 0));
                        }
                    }
                    cur = scope.parent().cloned();
                }
                let mut add = |snap: &mut Snapshot, name: &str, vars: Vec<Var>| {
                    snap.children.push(vars);
                    scopes.push(DbgScope {
                        name: name.to_string(),
                        reference: snap.children.len(),
                    });
                };
                add(&mut snap, "Locals", locals);
                if !globals.is_empty() {
                    add(&mut snap, "Globals", globals);
                }
            }
            snap.frames.push(Frame {
                name,
                file: loc.file,
                line: loc.line,
                column: loc.col,
                scopes,
            });
        }
        snap
    }

    fn var(&self, snap: &mut Snapshot, name: &str, v: &Value, depth: usize) -> Var {
        let (ty, value, members) = self.describe(v);
        let reference = if members.is_empty() || depth >= MAX_DEPTH {
            0
        } else {
            let mut list = Vec::new();
            for (n, m) in members.iter().take(MAX_MEMBERS) {
                list.push(self.var(snap, n, m, depth + 1));
            }
            if members.len() > MAX_MEMBERS {
                list.push(Var {
                    name: "…".into(),
                    value: format!("{} more", members.len() - MAX_MEMBERS),
                    ..Var::default()
                });
            }
            snap.children.push(list);
            snap.children.len()
        };
        Var {
            name: name.to_string(),
            ty,
            value,
            reference,
        }
    }

    /// The type, a short text and the parts of a value. Never runs program code.
    fn describe(&self, v: &Value) -> (String, String, Vec<(String, Value)>) {
        const SHOWN: usize = 80;
        let short = |s: String| {
            if s.chars().count() > SHOWN {
                format!("{}…", s.chars().take(SHOWN).collect::<String>())
            } else {
                s
            }
        };
        match v {
            Value::Unit => ("unit".into(), "()".into(), vec![]),
            Value::None => ("none".into(), "none".into(), vec![]),
            Value::Int(i) => ("int".into(), i.to_string(), vec![]),
            Value::Float(f) => ("float".into(), f.to_string(), vec![]),
            Value::Bool(b) => ("bool".into(), b.to_string(), vec![]),
            Value::Str(s) => ("string".into(), short(format!("{:?}", &**s)), vec![]),
            Value::Uuid(_) => ("uuid".into(), "uuid".into(), vec![]),
            Value::Ulid(_) => ("ulid".into(), "ulid".into(), vec![]),
            Value::DateTime(t) => ("datetime".into(), format!("{t} ms"), vec![]),
            Value::Bytes(b) => ("bytes".into(), format!("{} bytes", b.len()), vec![]),
            Value::Array(a) => {
                let items: Vec<(String, Value)> = a
                    .borrow()
                    .iter()
                    .take(MAX_MEMBERS + 1)
                    .enumerate()
                    .map(|(i, x)| (i.to_string(), x.clone()))
                    .collect();
                let n = a.borrow().len();
                ("array".into(), format!("{n} item(s)"), items)
            }
            Value::Map(m) => {
                let items: Vec<(String, Value)> = m
                    .borrow()
                    .entries
                    .iter()
                    .take(MAX_MEMBERS + 1)
                    .map(|(k, x)| (self.describe(k).1, x.clone()))
                    .collect();
                let n = m.borrow().entries.len();
                ("map".into(), format!("{n} entr(ies)"), items)
            }
            Value::Struct(o) => {
                let name = self.prog.defs.structs[o.def].name.clone();
                let items: Vec<(String, Value)> = o
                    .fields
                    .borrow()
                    .iter()
                    .map(|(k, x)| (k.to_string(), x.clone()))
                    .collect();
                (name.clone(), name, items)
            }
            Value::Tuple(t) => {
                let items = t
                    .iter()
                    .enumerate()
                    .map(|(i, x)| (i.to_string(), x.clone()))
                    .collect();
                ("tuple".into(), format!("{} item(s)", t.len()), items)
            }
            Value::Enum(eid, vi, data) => {
                let def = &self.prog.defs.enums[*eid as usize];
                let variant = &def.variants[*vi as usize];
                let items: Vec<(String, Value)> = match data {
                    Some(d) => variant
                        .fields
                        .iter()
                        .map(|(n, _)| n.clone())
                        .zip(d.iter().cloned())
                        .collect(),
                    None => vec![],
                };
                (
                    def.name.clone(),
                    format!("{}.{}", def.name, variant.name),
                    items,
                )
            }
            Value::Range(a, b) => ("range".into(), format!("{a}..{b}"), vec![]),
            Value::RangeIncl(a, b) => ("range".into(), format!("{a}..={b}"), vec![]),
            Value::Func(f) => ("function".into(), format!("fun {}", f.name), vec![]),
            Value::Any(a) => {
                let (t, s, m) = self.describe(&a.value);
                (format!("Any({t})"), s, m)
            }
        }
    }
}
