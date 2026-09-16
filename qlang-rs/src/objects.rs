use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{FunctionDecl, Visibility};
use crate::callable::Callable;
use crate::environment::Environment;
use crate::error::QError;
use crate::interpreter::Interpreter;
use crate::values::Value;

/// Runtime definition of a `structure ... avec ... fin` declaration, shared
/// by every instance and by the struct value itself (used for static calls
/// and to create raw instances via `Nom()`).
pub struct StructDef {
    pub name: String,
    pub fields: HashMap<String, Visibility>,
    /// Populated later by a matching `dans Nom implemente ... fin` block, so
    /// it needs interior mutability: the struct and its impl block are two
    /// separate statements evaluated in sequence.
    pub methods: RefCell<HashMap<String, Rc<MethodDef>>>,
}

/// A method attached to a struct via an `implemente` block.
pub struct MethodDef {
    pub owner: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub function: FunctionDecl,
    /// Environment in effect where the `implemente` block was evaluated,
    /// mirroring `QFunction`'s closure so methods can see outer/global
    /// declarations.
    pub closure: Environment,
}

/// A `Nom()`-created instance: struct fields with a shared, interior
/// mutable slot (assigning `instance.champ = ...` must be visible through
/// every other reference to the same instance).
pub struct Instance {
    pub struct_def: Rc<StructDef>,
    pub fields: RefCell<HashMap<String, Value>>,
}

/// A method bound to its receiver (`Some(instance)`) or left unbound for a
/// static call (`None`), callable like any other `Value::Function`.
pub struct BoundMethod {
    method: Rc<MethodDef>,
    receiver: Option<Value>,
}

impl BoundMethod {
    pub fn new(method: Rc<MethodDef>, receiver: Option<Value>) -> Self {
        Self { method, receiver }
    }
}

impl Callable for BoundMethod {
    fn arity(&self) -> usize {
        self.method.function.parameters.len()
    }

    fn call(&self, interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, QError> {
        let env = Environment::new(Some(self.method.closure.clone()));
        if let Some(receiver) = &self.receiver {
            env.declare_variable("moi", receiver.clone())?;
        }
        for (param, arg) in self.method.function.parameters.iter().zip(args) {
            env.declare_variable(param, arg)?;
        }

        interpreter.push_struct_context(self.method.owner.clone());
        let result = interpreter.evaluate_block(&self.method.function.body, env);
        interpreter.pop_struct_context();
        result
    }

    fn name(&self) -> String {
        format!(
            "{}.{}",
            self.method.owner,
            self.method.function.identifier.as_deref().unwrap_or("?")
        )
    }
}
