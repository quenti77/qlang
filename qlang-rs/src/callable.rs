use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ast::FunctionDecl;
use crate::environment::Environment;
use crate::error::QError;
use crate::interpreter::Interpreter;
use crate::values::Value;

/// Anything callable from qlang code: user-defined functions (`QFunction`)
/// as well as native/host-provided functions (see `globals`), so both can
/// live side by side as `Value::Function`.
pub trait Callable {
    fn arity(&self) -> usize;
    fn call(&self, interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, QError>;
    fn name(&self) -> String;
}

static ANON_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Resets the anonymous-function naming counter (`anon#1`, `anon#2`, ...).
/// Exposed so a fresh global environment produces deterministic names,
/// mirroring the TS `QFunction.counter = 0` reset in `makeGlobalEnv`.
pub fn reset_anon_counter() {
    ANON_COUNTER.store(0, Ordering::SeqCst);
}

pub struct QFunction {
    declaration: FunctionDecl,
    closure: Environment,
    name: String,
}

impl QFunction {
    pub fn new(declaration: FunctionDecl, closure: Environment) -> Self {
        let name = match &declaration.identifier {
            Some(name) => name.clone(),
            None => {
                let n = ANON_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
                format!("anon#{n}")
            }
        };
        Self {
            declaration,
            closure,
            name,
        }
    }
}

impl Callable for QFunction {
    fn arity(&self) -> usize {
        self.declaration.parameters.len()
    }

    fn call(&self, interpreter: &mut Interpreter, args: Vec<Value>) -> Result<Value, QError> {
        let env = Environment::new(Some(self.closure.clone()));
        for (param, arg) in self.declaration.parameters.iter().zip(args) {
            env.declare_variable(param, arg)?;
        }
        interpreter.evaluate_block(&self.declaration.body, env)
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}
