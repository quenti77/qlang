use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::QError;
use crate::values::Value;

struct Inner {
    parent: Option<Environment>,
    variables: HashMap<String, Value>,
}

/// A lexical scope. Cheaply cloneable (it's a handle around shared, interior
/// mutable state), matching how the interpreter swaps "current environment"
/// in and out as it enters/leaves blocks, loops and function calls.
#[derive(Clone)]
pub struct Environment(Rc<RefCell<Inner>>);

impl Environment {
    pub fn new(parent: Option<Environment>) -> Self {
        Environment(Rc::new(RefCell::new(Inner { parent, variables: HashMap::new() })))
    }

    pub fn declare_variable(&self, name: &str, value: Value) -> Result<Value, QError> {
        let mut inner = self.0.borrow_mut();
        if inner.variables.contains_key(name) {
            return Err(QError::runtime(format!("Variable '{name}' déjà déclarée")));
        }
        inner.variables.insert(name.to_string(), value.clone());
        Ok(value)
    }

    pub fn assign_variable(&self, name: &str, value: Value) -> Result<Value, QError> {
        let env = self
            .resolve(name, true)?
            .expect("resolve with throw_error=true always returns Some or errors");
        env.0.borrow_mut().variables.insert(name.to_string(), value.clone());
        Ok(value)
    }

    pub fn lookup_variable(&self, name: &str) -> Result<Value, QError> {
        let env = self
            .resolve(name, true)?
            .expect("resolve with throw_error=true always returns Some or errors");
        let value = env.0.borrow().variables.get(name).cloned().expect("resolved env must hold the variable");
        Ok(value)
    }

    pub fn resolve(&self, name: &str, throw_error: bool) -> Result<Option<Environment>, QError> {
        if self.0.borrow().variables.contains_key(name) {
            return Ok(Some(self.clone()));
        }

        let parent = self.0.borrow().parent.clone();
        if let Some(parent) = parent {
            return parent.resolve(name, throw_error);
        }

        if throw_error {
            return Err(QError::runtime(format!("Variable '{name}' non déclarée")));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declare_two_variables_with_different_names() {
        let env = Environment::new(None);
        env.declare_variable("a", Value::Number(42.0)).unwrap();
        env.declare_variable("b", Value::Number(43.0)).unwrap();

        assert_eq!(env.lookup_variable("a").unwrap(), Value::Number(42.0));
        assert_eq!(env.lookup_variable("b").unwrap(), Value::Number(43.0));
    }

    #[test]
    fn declare_two_variables_with_the_same_name() {
        let env = Environment::new(None);
        env.declare_variable("a", Value::Number(42.0)).unwrap();

        let err = env.declare_variable("a", Value::Number(43.0)).unwrap_err();
        assert_eq!(err.to_string(), "Erreur d'exécution: Variable 'a' déjà déclarée");
    }

    #[test]
    fn assign_a_value_to_an_undeclared_variable() {
        let env = Environment::new(None);
        let err = env.assign_variable("a", Value::Number(42.0)).unwrap_err();
        assert_eq!(err.to_string(), "Erreur d'exécution: Variable 'a' non déclarée");
    }

    #[test]
    fn assign_a_value_to_a_declared_variable() {
        let env = Environment::new(None);
        env.declare_variable("a", Value::Number(42.0)).unwrap();
        env.assign_variable("a", Value::Number(43.0)).unwrap();

        assert_eq!(env.lookup_variable("a").unwrap(), Value::Number(43.0));
    }

    #[test]
    fn lookup_an_undeclared_variable() {
        let env = Environment::new(None);
        let err = env.lookup_variable("a").unwrap_err();
        assert_eq!(err.to_string(), "Erreur d'exécution: Variable 'a' non déclarée");
    }

    #[test]
    fn lookup_a_declared_variable() {
        let env = Environment::new(None);
        env.declare_variable("a", Value::Number(42.0)).unwrap();

        assert_eq!(env.lookup_variable("a").unwrap(), Value::Number(42.0));
    }

    #[test]
    fn lookup_a_variable_declared_in_a_parent_environment() {
        let env = Environment::new(None);
        env.declare_variable("a", Value::Number(42.0)).unwrap();

        let child_env = Environment::new(Some(env));
        assert_eq!(child_env.lookup_variable("a").unwrap(), Value::Number(42.0));
    }
}
