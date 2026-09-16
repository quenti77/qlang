use std::cell::RefCell;
use std::rc::Rc;

use crate::callable::Callable;
use crate::objects::{Instance, StructDef};

/// qlang arrays are reference types (as in the original JS/TS interpreter):
/// two variables pointing at "the same" array must observe each other's
/// mutations, hence the shared, interior-mutable storage.
pub type ArrayRef = Rc<RefCell<Vec<Value>>>;

#[derive(Clone)]
pub enum Value {
    Null,
    Number(f64),
    Boolean(bool),
    String(String),
    Array(ArrayRef),
    Function(Rc<dyn Callable>),
    /// The struct itself (e.g. `Nom`), used to call it as a raw constructor
    /// (`Nom()`) or to call a static method (`Nom.methode()`).
    Struct(Rc<StructDef>),
    /// An instance created via `Nom()`.
    Instance(Rc<Instance>),
    Break,
    Continue,
    Return(Box<Value>),
}

impl Value {
    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Rc::new(RefCell::new(items)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Number(_) => "number",
            Value::Boolean(_) => "boolean",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Function(_) => "function",
            Value::Struct(_) => "structure",
            Value::Instance(_) => "instance",
            Value::Break => "break",
            Value::Continue => "continue",
            Value::Return(_) => "return",
        }
    }

    /// JS-style truthiness, used by `si`/`tantque`/`pour` conditions:
    /// everything is truthy except `rien`, `faux`, `0`, `NaN` and `""`.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::Boolean(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::Array(_) | Value::Function(_) | Value::Struct(_) | Value::Instance(_) => true,
            Value::Break | Value::Continue => false,
            Value::Return(inner) => inner.is_truthy(),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => *a.borrow() == *b.borrow(),
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Struct(a), Value::Struct(b)) => Rc::ptr_eq(a, b),
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Break, Value::Break) => true,
            (Value::Continue, Value::Continue) => true,
            (Value::Return(a), Value::Return(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "Null"),
            Value::Number(n) => write!(f, "Number({n})"),
            Value::Boolean(b) => write!(f, "Boolean({b})"),
            Value::String(s) => write!(f, "String({s:?})"),
            Value::Array(items) => write!(f, "Array({:?})", items.borrow()),
            Value::Function(func) => write!(f, "Function({})", func.name()),
            Value::Struct(def) => write!(f, "Struct({})", def.name),
            Value::Instance(instance) => write!(f, "Instance({})", instance.struct_def.name),
            Value::Break => write!(f, "Break"),
            Value::Continue => write!(f, "Continue"),
            Value::Return(inner) => write!(f, "Return({inner:?})"),
        }
    }
}
