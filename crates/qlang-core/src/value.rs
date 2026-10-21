//! Runtime values and environments.

use crate::ast::FunDecl;
use crate::smallmap::SmallMap;
use crate::types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A 128-bit identifier, as two halves so that a `Value` stays small.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id(u64, u64);

impl Id {
    pub fn new(v: u128) -> Id {
        Id((v >> 64) as u64, v as u64)
    }

    pub fn get(self) -> u128 {
        (u128::from(self.0) << 64) | u128::from(self.1)
    }
}

#[derive(Clone)]
pub enum Value {
    /// The result of expressions without value.
    Unit,
    None,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),
    Uuid(Id),
    Ulid(Id),
    /// Milliseconds since 1970 (UTC).
    DateTime(i64),
    /// Arrays and structs are shared: copying a value copies the reference.
    Array(Rc<RefCell<Vec<Value>>>),
    /// Keys keep their insertion order.
    Map(Rc<RefCell<MapObj>>),
    Struct(Rc<StructObj>),
    /// A tuple: a fixed number of values, never changed.
    Tuple(Rc<[Value]>),
    /// An enum value: its type, the index of its variant and the data it carries (values
    /// are never changed, so a copy is just another handle).
    Enum(u32, u32, Option<Rc<Vec<Value>>>),
    /// `start..end`
    Range(i64, i64),
    /// `start..=end`
    RangeIncl(i64, i64),
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
    Tuple(Vec<KeyVal>),
}

impl KeyVal {
    pub fn of(v: &Value) -> Option<KeyVal> {
        Some(match v {
            Value::Int(i) => KeyVal::Int(*i),
            Value::Float(f) => KeyVal::Float(Self::float_bits(*f)),
            Value::Str(s) => KeyVal::Str(s.clone()),
            Value::Bool(b) => KeyVal::Bool(*b),
            Value::Uuid(u) => KeyVal::Uuid(u.get()),
            Value::Ulid(u) => KeyVal::Ulid(u.get()),
            Value::DateTime(t) => KeyVal::DateTime(*t),
            Value::Enum(e, i, None) => KeyVal::Enum(*e as usize, *i as usize),
            Value::Tuple(items) => {
                KeyVal::Tuple(items.iter().map(KeyVal::of).collect::<Option<Vec<_>>>()?)
            }
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
    pub fields: RefCell<SmallMap<Value>>,
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
    vars: RefCell<SmallMap<Value>>,
    parent: Option<Env>,
}

pub type Env = Rc<Scope>;

/// "Not cached yet" for [`Scope::get_cached`].
pub const NO_SLOT: u32 = u32::MAX;

impl Scope {
    pub fn new(parent: Option<Env>) -> Env {
        Rc::new(Scope {
            vars: RefCell::new(SmallMap::default()),
            parent,
        })
    }

    /// A scope with room for `n` variables (a call knows how many it will define).
    pub fn with_capacity(parent: Option<Env>, n: usize) -> Env {
        Rc::new(Scope {
            vars: RefCell::new(SmallMap::with_capacity(n)),
            parent,
        })
    }

    pub fn define(&self, name: &str, v: Value) {
        self.vars.borrow_mut().insert(name, v);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let mut cur: &Scope = self;
        loop {
            if let Some(v) = cur.vars.borrow().get(name) {
                return Some(v.clone());
            }
            cur = cur.parent.as_ref()?;
        }
    }

    /// Like [`Scope::get`], remembering in `cache` where the name was found (scopes up, slot):
    /// the same place in the program finds its variable in the same place again, so the next
    /// lookup goes straight there. A cache that no longer fits is simply ignored.
    #[inline]
    pub fn get_cached(&self, name: &str, cache: &mut u32) -> Option<Value> {
        let c = *cache;
        if c != NO_SLOT {
            let mut cur: &Scope = self;
            let mut hops = c >> 16;
            while hops > 0 {
                match &cur.parent {
                    Some(p) => cur = p,
                    None => break,
                }
                hops -= 1;
            }
            if hops == 0
                && let Some(v) = cur.vars.borrow().get_at((c & 0xFFFF) as usize, name)
            {
                return Some(v.clone());
            }
        }
        self.find_slot(name, cache)
    }

    #[inline(never)]
    fn find_slot(&self, name: &str, cache: &mut u32) -> Option<Value> {
        let mut cur: &Scope = self;
        let mut hops = 0u32;
        loop {
            {
                let vars = cur.vars.borrow();
                if let Some(i) = vars.position(name) {
                    if hops < 0xFFFF && i < 0xFFFF {
                        *cache = (hops << 16) | i as u32;
                    }
                    return vars.get_at(i, name).cloned();
                }
            }
            cur = cur.parent.as_ref()?;
            hops += 1;
        }
    }

    /// [`Scope::assign`] with the cache of [`Scope::get_cached`].
    #[inline]
    pub fn assign_cached(&self, name: &str, v: Value, cache: &mut u32) -> bool {
        let c = *cache;
        if c != NO_SLOT {
            let mut cur: &Scope = self;
            let mut hops = c >> 16;
            while hops > 0 {
                match &cur.parent {
                    Some(p) => cur = p,
                    None => break,
                }
                hops -= 1;
            }
            if hops == 0
                && let Some(slot) = cur
                    .vars
                    .borrow_mut()
                    .get_mut_at((c & 0xFFFF) as usize, name)
            {
                *slot = v;
                return true;
            }
        }
        // slow path: find the slot, then assign
        let mut cur: &Scope = self;
        let mut hops = 0u32;
        loop {
            {
                let mut vars = cur.vars.borrow_mut();
                if let Some(i) = vars.position(name) {
                    if hops < 0xFFFF && i < 0xFFFF {
                        *cache = (hops << 16) | i as u32;
                    }
                    if let Some(slot) = vars.get_mut_at(i, name) {
                        *slot = v;
                    }
                    return true;
                }
            }
            match &cur.parent {
                Some(p) => {
                    cur = p;
                    hops += 1;
                }
                None => return false,
            }
        }
    }

    /// Assign to an existing variable, in the nearest scope that has it.
    pub fn assign(&self, name: &str, v: Value) -> bool {
        let mut cur: &Scope = self;
        loop {
            if let Some(slot) = cur.vars.borrow_mut().get_mut(name) {
                *slot = v;
                return true;
            }
            match &cur.parent {
                Some(p) => cur = p,
                None => return false,
            }
        }
    }
}
