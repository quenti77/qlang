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
    /// Arrays and structs are shared: copying a value copies the reference.
    Array(Rc<RefCell<Vec<Value>>>),
    Struct(Rc<StructObj>),
    Enum(EnumId, usize),
    /// `start..end` or `start..=end` (the flag is "inclusive").
    Range(i64, i64, bool),
    Func(Rc<FuncVal>),
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
        Rc::new(Scope { vars: RefCell::new(HashMap::new()), parent })
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
