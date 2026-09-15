use std::rc::Rc;

use crate::callable::{reset_anon_counter, Callable};
use crate::environment::Environment;
use crate::error::QError;
use crate::interpreter::Interpreter;
use crate::values::Value;

struct TailleFunction;

impl Callable for TailleFunction {
    fn arity(&self) -> usize {
        1
    }

    fn call(&self, _interpreter: &mut Interpreter, mut args: Vec<Value>) -> Result<Value, QError> {
        match args.remove(0) {
            Value::Array(items) => Ok(Value::Number(items.borrow().len() as f64)),
            other => Err(QError::runtime(format!(
                "'taille' attend un tableau, reçu '{}'",
                other.type_name()
            ))),
        }
    }

    fn name(&self) -> String {
        "taille".to_string()
    }
}

/// Builds a fresh global environment pre-populated with the standard
/// library available to every qlang program (currently just `taille`).
pub fn make_global_env() -> Environment {
    reset_anon_counter();

    let env = Environment::new(None);
    env.declare_variable("taille", Value::Function(Rc::new(TailleFunction)))
        .expect("declaring a builtin in a fresh environment cannot fail");

    env
}
