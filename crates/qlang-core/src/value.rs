//! Runtime values and environments.

use crate::ast::FunDecl;
use crate::types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone)]
pub enum Value {
    /// The result of expressions without value.
    Unit,
    None,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),
    Uuid(u128),
    Ulid(u128),
    /// Milliseconds since 1970 (UTC).
    DateTime(i64),
    /// Arrays and structs are shared: copying a value copies the reference.
    Array(Rc<RefCell<Vec<Value>>>),
    /// Keys keep their insertion order.
    Map(Rc<RefCell<MapObj>>),
    Struct(Rc<StructObj>),
    Enum(EnumId, usize),
    /// `start..end` or `start..=end` (the flag is "inclusive").
    Range(i64, i64, bool),
    Func(Rc<FuncVal>),
}

/// A map key: only types compared by value can be keys.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum KeyVal {
    Int(i64),
    /// IEEE-754 bits, with `-0.0` normalized to `0.0` and any `NaN` normalized
    /// to a single canonical bit pattern so every `NaN` key collides with itself.
    Float(u64),
    Str(Rc<str>),
    Bool(bool),
    Uuid(u128),
    Ulid(u128),
    DateTime(i64),
    Enum(EnumId, usize),
}

impl KeyVal {
    pub fn of(v: &Value) -> Option<KeyVal> {
        Some(match v {
            Value::Int(i) => KeyVal::Int(*i),
            Value::Float(f) => KeyVal::Float(Self::float_bits(*f)),
            Value::Str(s) => KeyVal::Str(s.clone()),
            Value::Bool(b) => KeyVal::Bool(*b),
            Value::Uuid(u) => KeyVal::Uuid(*u),
            Value::Ulid(u) => KeyVal::Ulid(*u),
            Value::DateTime(t) => KeyVal::DateTime(*t),
            Value::Enum(e, i) => KeyVal::Enum(*e, *i),
            _ => return None,
        })
    }

    fn float_bits(f: f64) -> u64 {
        if f.is_nan() {
            f64::NAN.to_bits()
        } else if f == 0.0 {
            0.0f64.to_bits()
        } else {
            f.to_bits()
        }
    }
}

/// An insertion-ordered map.
#[derive(Default)]
pub struct MapObj {
    pub entries: Vec<(Value, Value)>,
    index: HashMap<KeyVal, usize>,
}

impl MapObj {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, k: &Value) -> Option<&Value> {
        let i = *self.index.get(&KeyVal::of(k)?)?;
        Some(&self.entries[i].1)
    }

    /// Insert or replace; false if `k` cannot be a key.
    pub fn insert(&mut self, k: Value, v: Value) -> bool {
        let Some(key) = KeyVal::of(&k) else {
            return false;
        };
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                self.index.insert(key, self.entries.len());
                self.entries.push((k, v));
            }
        }
        true
    }

    pub fn remove(&mut self, k: &Value) -> Option<Value> {
        let i = self.index.remove(&KeyVal::of(k)?)?;
        let (_, v) = self.entries.remove(i);
        for slot in self.index.values_mut() {
            if *slot > i {
                *slot -= 1;
            }
        }
        Some(v)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }
}

pub struct StructObj {
    pub def: StructId,
    pub fields: RefCell<HashMap<String, Value>>,
}

pub struct FuncVal {
    pub decl: Rc<FunDecl>,
    pub env: Env,
    pub name: String,
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(items)))
    }
}

/// A lexical scope. Closures keep their scope alive.
pub struct Scope {
    vars: RefCell<HashMap<String, Value>>,
    parent: Option<Env>,
}

pub type Env = Rc<Scope>;

impl Scope {
    pub fn new(parent: Option<Env>) -> Env {
        Rc::new(Scope {
            vars: RefCell::new(HashMap::new()),
            parent,
        })
    }

    pub fn define(&self, name: &str, v: Value) {
        self.vars.borrow_mut().insert(name.to_string(), v);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.vars.borrow().get(name) {
            return Some(v.clone());
        }
        let mut cur = self.parent.clone();
        while let Some(s) = cur {
            if let Some(v) = s.vars.borrow().get(name) {
                return Some(v.clone());
            }
            cur = s.parent.clone();
        }
        None
    }

    /// Assign to an existing variable, in the nearest scope that has it.
    pub fn assign(&self, name: &str, v: Value) -> bool {
        if let Some(slot) = self.vars.borrow_mut().get_mut(name) {
            *slot = v;
            return true;
        }
        let mut cur = self.parent.clone();
        while let Some(s) = cur {
            if let Some(slot) = s.vars.borrow_mut().get_mut(name) {
                *slot = v;
                return true;
            }
            cur = s.parent.clone();
        }
        false
    }
}
