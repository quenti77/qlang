use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    Expr, FunctionDecl, MatchArm, MethodDecl, Program, Stmt, StructField, Visibility,
};
use crate::callable::{Callable, QFunction};
use crate::environment::Environment;
use crate::error::QError;
use crate::lexer::Lexer;
use crate::module::{InputSource, ModuleResolver};
use crate::objects::{BoundMethod, FieldDef, Instance, MethodDef, StructDef};
use crate::parser::Parser;
use crate::stdio::Std;
use crate::values::Value;

pub struct Interpreter {
    env: Environment,
    stdout: Std,
    stderr: Std,
    input: Box<dyn InputSource>,
    module_resolver: Box<dyn ModuleResolver>,
    included_modules: HashSet<String>,
    /// Names of the structs whose method body is currently executing,
    /// innermost last - used to gate access to `cacher`/`partager` members
    /// to code running inside a method of that same struct.
    struct_context: Vec<String>,
}

impl Interpreter {
    pub fn new(
        env: Environment,
        stdout: Std,
        stderr: Std,
        input: Box<dyn InputSource>,
        module_resolver: Box<dyn ModuleResolver>,
    ) -> Self {
        Self {
            env,
            stdout,
            stderr,
            input,
            module_resolver,
            included_modules: HashSet::new(),
            struct_context: Vec::new(),
        }
    }

    pub fn push_struct_context(&mut self, owner: String) {
        self.struct_context.push(owner);
    }

    pub fn pop_struct_context(&mut self) {
        self.struct_context.pop();
    }

    fn is_in_struct_context(&self, owner: &str) -> bool {
        self.struct_context.last().map(|s| s.as_str()) == Some(owner)
    }

    fn check_visibility(
        &self,
        visibility: Visibility,
        owner: &str,
        member: &str,
    ) -> Result<(), QError> {
        if visibility == Visibility::Public || self.is_in_struct_context(owner) {
            return Ok(());
        }
        Err(QError::runtime(format!(
            "'{member}' n'est pas accessible depuis l'extérieur de '{owner}'"
        )))
    }

    pub fn stdout(&self) -> &Std {
        &self.stdout
    }

    pub fn stderr(&self) -> &Std {
        &self.stderr
    }

    pub fn environment(&self) -> &Environment {
        &self.env
    }

    pub fn evaluate(&mut self, program: &Program) -> Result<Value, QError> {
        let mut last = Value::Null;
        for stmt in program {
            last = self.evaluate_stmt(stmt)?;
        }
        Ok(last)
    }

    /// Runs `body` inside `environment`, restoring the interpreter's
    /// previous current environment afterwards no matter how execution
    /// ends (normal completion, `arreter`/`continuer`/`retour`, or error).
    /// Used both for block statements (if/while/for bodies) and for
    /// function calls.
    pub fn evaluate_block(
        &mut self,
        body: &[Stmt],
        environment: Environment,
    ) -> Result<Value, QError> {
        let previous_env = std::mem::replace(&mut self.env, environment);
        let result = self.run_block_body(body);
        self.env = previous_env;
        result
    }

    /// Evaluates `expr` with `environment` as the current scope, restoring
    /// the previous one afterwards - used to evaluate a struct field's
    /// default value in the scope where the struct was declared, rather
    /// than the caller's scope.
    fn evaluate_expr_in_env(
        &mut self,
        expr: &Expr,
        environment: Environment,
    ) -> Result<Value, QError> {
        let previous_env = std::mem::replace(&mut self.env, environment);
        let result = self.evaluate_expr(expr);
        self.env = previous_env;
        result
    }

    fn run_block_body(&mut self, body: &[Stmt]) -> Result<Value, QError> {
        let mut last = Value::Null;
        for stmt in body {
            match stmt {
                Stmt::Break => return Ok(Value::Break),
                Stmt::Continue => return Ok(Value::Continue),
                Stmt::Return(expr) => {
                    let result = self.evaluate_expr(expr)?;
                    return Ok(Value::Return(Box::new(result)));
                }
                _ => {}
            }

            last = self.evaluate_stmt(stmt)?;
            if matches!(last, Value::Break | Value::Continue | Value::Return(_)) {
                return Ok(last);
            }
        }
        Ok(last)
    }

    fn evaluate_in_new_scope(&mut self, stmt: &Stmt) -> Result<Value, QError> {
        let child = Environment::new(Some(self.env.clone()));
        let previous = std::mem::replace(&mut self.env, child);
        let result = self.evaluate_stmt(stmt);
        self.env = previous;
        result
    }

    fn evaluate_stmt(&mut self, stmt: &Stmt) -> Result<Value, QError> {
        match stmt {
            Stmt::VariableDeclaration {
                identifier,
                value,
                is_const,
            } => self.evaluate_variable_declaration(identifier, value.as_ref(), *is_const),
            Stmt::Print(expr) => self.evaluate_print(expr),
            Stmt::Block(body) => self.run_block_body(body),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => self.evaluate_if(condition, then_branch, else_branch.as_deref()),
            Stmt::While { condition, body } => self.evaluate_while(condition, body),
            Stmt::For {
                identifier,
                from,
                until,
                step,
                body,
            } => self.evaluate_for(identifier, from, until, step, body),
            Stmt::Function(decl) => self.evaluate_function_declaration(decl),
            Stmt::Break => Ok(Value::Break),
            Stmt::Continue => Ok(Value::Continue),
            Stmt::Return(expr) => Ok(Value::Return(Box::new(self.evaluate_expr(expr)?))),
            Stmt::Include(expr) => self.evaluate_include(expr),
            Stmt::Struct { name, fields } => self.evaluate_struct_declaration(name, fields),
            Stmt::Impl { name, methods } => self.evaluate_impl_declaration(name, methods),
            Stmt::Match {
                subject,
                arms,
                default,
            } => self.evaluate_match(subject, arms, default.as_deref()),
            Stmt::Expr(expr) => self.evaluate_expr(expr),
        }
    }

    fn evaluate_match(
        &mut self,
        subject: &Expr,
        arms: &[MatchArm],
        default: Option<&Stmt>,
    ) -> Result<Value, QError> {
        let subject_value = self.evaluate_expr(subject)?;

        for arm in arms {
            let pattern_value = self.evaluate_expr(&arm.pattern)?;
            if values_equal(&subject_value, &pattern_value) {
                return self.evaluate_in_new_scope(&arm.body);
            }
        }

        match default {
            Some(default) => self.evaluate_in_new_scope(default),
            None => Ok(Value::Null),
        }
    }

    fn evaluate_struct_declaration(
        &mut self,
        name: &str,
        fields: &[StructField],
    ) -> Result<Value, QError> {
        let mut field_map = HashMap::new();
        for field in fields {
            field_map.insert(
                field.name.clone(),
                FieldDef {
                    visibility: field.visibility,
                    default: field.default.clone(),
                },
            );
        }

        let value = Value::Struct(Rc::new(StructDef {
            name: name.to_string(),
            fields: field_map,
            methods: std::cell::RefCell::new(HashMap::new()),
            closure: self.env.clone(),
        }));

        if self.env.resolve(name, false)?.is_none() {
            self.env.declare_variable(name, Value::Null)?;
        }
        self.env.assign_variable(name, value)
    }

    fn evaluate_impl_declaration(
        &mut self,
        name: &str,
        methods: &[MethodDecl],
    ) -> Result<Value, QError> {
        let def = match self.env.lookup_variable(name)? {
            Value::Struct(def) => def,
            other => {
                return Err(QError::runtime(format!(
                    "'{name}' n'est pas une structure, reçu '{}'",
                    other.type_name()
                )))
            }
        };

        for method in methods {
            let method_name = method
                .function
                .identifier
                .clone()
                .expect("method declarations always carry an identifier");

            if def.fields.contains_key(&method_name) {
                return Err(QError::runtime(format!(
                    "'{method_name}' est déjà utilisé comme champ de '{name}', choisissez un autre nom pour la méthode"
                )));
            }

            let method_def = Rc::new(MethodDef {
                owner: name.to_string(),
                visibility: method.visibility,
                is_static: method.is_static,
                function: method.function.clone(),
                closure: self.env.clone(),
            });

            def.methods.borrow_mut().insert(method_name, method_def);
        }

        Ok(Value::Null)
    }

    fn evaluate_variable_declaration(
        &mut self,
        identifier: &str,
        value: Option<&Expr>,
        is_const: bool,
    ) -> Result<Value, QError> {
        match value {
            Some(expr) => {
                let evaluated = self.evaluate_expr(expr)?;
                if is_const {
                    self.env.declare_constant(identifier, evaluated.clone())?;
                } else {
                    self.env.declare_variable(identifier, evaluated.clone())?;
                }
                Ok(evaluated)
            }
            None => {
                self.env.declare_variable(identifier, Value::Null)?;
                Ok(Value::Null)
            }
        }
    }

    fn evaluate_print(&mut self, expr: &Expr) -> Result<Value, QError> {
        let value = self.evaluate_expr(expr)?;
        let text = display_value(&value);
        self.stdout.print(text);
        Ok(Value::Null)
    }

    fn evaluate_if(
        &mut self,
        condition: &Expr,
        then_branch: &Stmt,
        else_branch: Option<&Stmt>,
    ) -> Result<Value, QError> {
        let condition = self.evaluate_expr(condition)?;

        if condition.is_truthy() {
            self.evaluate_in_new_scope(then_branch)
        } else if let Some(else_branch) = else_branch {
            self.evaluate_in_new_scope(else_branch)
        } else {
            Ok(Value::Null)
        }
    }

    fn evaluate_while(&mut self, condition: &Expr, body: &Stmt) -> Result<Value, QError> {
        let child = Environment::new(Some(self.env.clone()));
        let previous = std::mem::replace(&mut self.env, child);

        let result = (|| -> Result<Value, QError> {
            while self.evaluate_expr(condition)?.is_truthy() {
                let result = self.evaluate_stmt(body)?;
                if matches!(result, Value::Break) {
                    break;
                }
                if matches!(result, Value::Return(_)) {
                    return Ok(result);
                }
            }
            Ok(Value::Null)
        })();

        self.env = previous;
        result
    }

    fn evaluate_for(
        &mut self,
        identifier: &str,
        from: &Expr,
        until: &Expr,
        step: &Expr,
        body: &Stmt,
    ) -> Result<Value, QError> {
        let child = Environment::new(Some(self.env.clone()));
        let previous = std::mem::replace(&mut self.env, child);

        let result = (|| -> Result<Value, QError> {
            if self.env.resolve(identifier, false)?.is_none() {
                self.env.declare_variable(identifier, Value::Null)?;
            }
            let from_value = self.evaluate_expr(from)?;
            self.env.assign_variable(identifier, from_value)?;

            while self.evaluate_expr(until)?.is_truthy() {
                let result = self.evaluate_stmt(body)?;
                if matches!(result, Value::Break) {
                    break;
                }
                if matches!(result, Value::Return(_)) {
                    return Ok(result);
                }
                self.evaluate_expr(step)?;
            }
            Ok(Value::Null)
        })();

        self.env = previous;
        result
    }

    fn evaluate_function_declaration(&mut self, decl: &FunctionDecl) -> Result<Value, QError> {
        let qfunction = QFunction::new(decl.clone(), self.env.clone());
        let name = qfunction.name();
        let value = Value::Function(Rc::new(qfunction));

        if self.env.resolve(&name, false)?.is_none() {
            self.env.declare_variable(&name, Value::Null)?;
        }
        self.env.assign_variable(&name, value)
    }

    fn evaluate_include(&mut self, expr: &Expr) -> Result<Value, QError> {
        let path_value = self.evaluate_expr(expr)?;
        let path = match path_value {
            Value::String(s) => s,
            other => {
                return Err(QError::module(format!(
                    "'inclure' attend un chemin de fichier (chaîne), reçu '{}'",
                    other.type_name()
                )))
            }
        };

        if !self.included_modules.insert(path.clone()) {
            return Ok(Value::Null);
        }

        let source = self.module_resolver.resolve(&path)?;

        let mut lexer = Lexer::new();
        lexer.tokenize(&source)?;
        let mut parser = Parser::new();
        parser.set_tokens(lexer.tokens().to_vec(), source);
        let program = parser.make_ast()?;

        self.evaluate(&program)
    }

    fn evaluate_expr(&mut self, expr: &Expr) -> Result<Value, QError> {
        match expr {
            Expr::Assignment { target, value } => self.evaluate_assignment(target, value),
            Expr::Read(value) => self.evaluate_read(value),
            Expr::Unary { operator, value } => self.evaluate_unary(operator, value),
            Expr::Binary {
                left,
                right,
                operator,
            } => self.evaluate_binary(left, right, operator),
            Expr::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.evaluate_expr(element)?);
                }
                Ok(Value::array(values))
            }
            Expr::Member { object, property } => self.evaluate_member(object, property.as_deref()),
            Expr::Call { callee, arguments } => self.evaluate_call(callee, arguments),
            Expr::Identifier(name) => self.env.lookup_variable(name),
            Expr::Numeric(n) => Ok(Value::Number(*n)),
            Expr::Str(s) => Ok(Value::String(s.clone())),
            Expr::Null => Ok(Value::Null),
            Expr::Boolean(b) => Ok(Value::Boolean(*b)),
            Expr::Function(decl) => self.evaluate_function_declaration(decl),
        }
    }

    fn evaluate_assignment(&mut self, target: &Expr, value: &Expr) -> Result<Value, QError> {
        match target {
            Expr::Identifier(name) => {
                let evaluated = self.evaluate_expr(value)?;
                self.env.assign_variable(name, evaluated)
            }
            Expr::Member { object, property } => {
                let object_value = self.evaluate_expr(object)?;

                match object_value {
                    Value::Array(array) => match property {
                        None => {
                            let evaluated = self.evaluate_expr(value)?;
                            array.borrow_mut().push(evaluated.clone());
                            Ok(evaluated)
                        }
                        Some(property_expr) => {
                            let index_value = self.evaluate_expr(property_expr)?;
                            let index = to_index(&index_value)?;
                            if index >= array.borrow().len() {
                                return Err(QError::runtime(format!(
                                    "Index '{index}' hors limites"
                                )));
                            }
                            let evaluated = self.evaluate_expr(value)?;
                            array.borrow_mut()[index] = evaluated.clone();
                            Ok(evaluated)
                        }
                    },
                    Value::Instance(instance) => {
                        let name = self.evaluate_member_name(property.as_deref())?;
                        let visibility = instance
                            .struct_def
                            .fields
                            .get(&name)
                            .ok_or_else(|| {
                                QError::runtime(format!(
                                    "'{}' n'a pas de champ '{name}'",
                                    instance.struct_def.name
                                ))
                            })?
                            .visibility;
                        self.check_visibility(visibility, &instance.struct_def.name, &name)?;

                        let evaluated = self.evaluate_expr(value)?;
                        instance.fields.borrow_mut().insert(name, evaluated.clone());
                        Ok(evaluated)
                    }
                    other => Err(QError::runtime(format!(
                        "Impossible d'assigner un élément d'une valeur de type '{}'",
                        other.type_name()
                    ))),
                }
            }
            _ => Err(QError::runtime("Cible d'affectation invalide")),
        }
    }

    fn evaluate_read(&mut self, value: &Expr) -> Result<Value, QError> {
        let message_value = self.evaluate_expr(value)?;
        let message = display_value(&message_value);

        Ok(match self.input.read(&message) {
            Some(s) => Value::String(s),
            None => Value::Null,
        })
    }

    fn evaluate_unary(&mut self, operator: &str, value: &Expr) -> Result<Value, QError> {
        let argument = self.evaluate_expr(value)?;
        Ok(match operator {
            "-" => Value::Number(-to_number(&argument)),
            "non" => Value::Boolean(!argument.is_truthy()),
            _ => {
                return Err(QError::runtime(format!(
                    "Opérateur unaire '{operator}' inconnu"
                )))
            }
        })
    }

    fn evaluate_member(&mut self, object: &Expr, property: Option<&Expr>) -> Result<Value, QError> {
        let object_value = self.evaluate_expr(object)?;

        match object_value {
            Value::Array(items) => {
                let property_expr = property
                    .ok_or_else(|| QError::runtime("Accès à un élément du tableau sans index"))?;
                let index_value = self.evaluate_expr(property_expr)?;
                let index = to_index(&index_value)?;

                let borrowed = items.borrow();
                if index >= borrowed.len() {
                    return Err(QError::runtime(format!("Index '{index}' hors limites")));
                }
                Ok(borrowed[index].clone())
            }
            Value::Instance(instance) => {
                let name = self.evaluate_member_name(property)?;
                self.access_instance_member(&instance, &name)
            }
            Value::Struct(def) => {
                let name = self.evaluate_member_name(property)?;
                self.access_static_member(&def, &name)
            }
            other => Err(QError::runtime(format!(
                "Impossible d'accéder à un élément d'une valeur de type '{}'",
                other.type_name()
            ))),
        }
    }

    fn evaluate_member_name(&mut self, property: Option<&Expr>) -> Result<String, QError> {
        let property_expr =
            property.ok_or_else(|| QError::runtime("Accès à un membre sans nom"))?;
        match self.evaluate_expr(property_expr)? {
            Value::String(name) => Ok(name),
            other => Err(QError::runtime(format!(
                "Nom de membre invalide, reçu '{}'",
                other.type_name()
            ))),
        }
    }

    fn access_instance_member(
        &mut self,
        instance: &Rc<Instance>,
        name: &str,
    ) -> Result<Value, QError> {
        if let Some(value) = instance.fields.borrow().get(name) {
            let visibility = instance
                .struct_def
                .fields
                .get(name)
                .expect("field exists in fields map")
                .visibility;
            self.check_visibility(visibility, &instance.struct_def.name, name)?;
            return Ok(value.clone());
        }

        if let Some(method) = instance.struct_def.methods.borrow().get(name) {
            if method.is_static {
                return Err(QError::runtime(format!(
                    "'{name}' est une méthode statique, appelez-la via '{}.{name}'",
                    instance.struct_def.name
                )));
            }
            self.check_visibility(method.visibility, &instance.struct_def.name, name)?;
            return Ok(Value::Function(Rc::new(BoundMethod::new(
                method.clone(),
                Some(Value::Instance(instance.clone())),
            ))));
        }

        Err(QError::runtime(format!(
            "'{}' n'a pas de membre '{name}'",
            instance.struct_def.name
        )))
    }

    fn access_static_member(&mut self, def: &Rc<StructDef>, name: &str) -> Result<Value, QError> {
        if let Some(method) = def.methods.borrow().get(name) {
            if !method.is_static {
                return Err(QError::runtime(format!(
                    "'{name}' n'est pas une méthode statique de '{}'",
                    def.name
                )));
            }
            self.check_visibility(method.visibility, &def.name, name)?;
            return Ok(Value::Function(Rc::new(BoundMethod::new(
                method.clone(),
                None,
            ))));
        }

        Err(QError::runtime(format!(
            "'{}' n'a pas de méthode statique '{name}'",
            def.name
        )))
    }

    fn evaluate_call(&mut self, callee: &Expr, arguments: &[Expr]) -> Result<Value, QError> {
        let callee_value = self.evaluate_expr(callee)?;

        if let Value::Struct(def) = callee_value {
            if !arguments.is_empty() {
                return Err(QError::runtime(format!(
                    "'{}' ne prend aucun argument (utilisez une méthode statique pour un constructeur personnalisé)",
                    def.name
                )));
            }
            let mut fields = HashMap::new();
            for (name, field_def) in def.fields.iter() {
                let value = match &field_def.default {
                    Some(default_expr) => {
                        self.evaluate_expr_in_env(default_expr, def.closure.clone())?
                    }
                    None => Value::Null,
                };
                fields.insert(name.clone(), value);
            }
            return Ok(Value::Instance(Rc::new(Instance {
                struct_def: def,
                fields: std::cell::RefCell::new(fields),
            })));
        }

        let function = match callee_value {
            Value::Function(f) => f,
            other => {
                return Err(QError::runtime(format!(
                    "'{}' n'est pas une fonction",
                    other.type_name()
                )))
            }
        };

        if arguments.len() != function.arity() {
            return Err(QError::runtime(format!(
                "La fonction '{}' attend {} argument(s), {} reçu(s)",
                function.name(),
                function.arity(),
                arguments.len()
            )));
        }

        let mut args = Vec::with_capacity(arguments.len());
        for argument in arguments {
            args.push(self.evaluate_expr(argument)?);
        }

        let result = function.call(self, args)?;
        Ok(match result {
            Value::Return(inner) => *inner,
            other => other,
        })
    }

    fn evaluate_binary(
        &mut self,
        left: &Expr,
        right: &Expr,
        operator: &str,
    ) -> Result<Value, QError> {
        if is_logical_operator(operator) {
            return self.evaluate_logical_binary(left, right, operator);
        }

        let left_value = self.evaluate_expr(left)?;
        let right_value = self.evaluate_expr(right)?;
        evaluate_algebraic_binary(operator, &left_value, &right_value)
    }

    fn evaluate_logical_binary(
        &mut self,
        left: &Expr,
        right: &Expr,
        operator: &str,
    ) -> Result<Value, QError> {
        let left_value = self.evaluate_expr(left)?;

        if operator == "et" {
            if let Value::Boolean(false) = left_value {
                return Ok(Value::Boolean(false));
            }
            let right_value = self.evaluate_expr(right)?;
            return Ok(Value::Boolean(right_value.is_truthy()));
        }
        if operator == "ou" {
            if let Value::Boolean(true) = left_value {
                return Ok(Value::Boolean(true));
            }
            let right_value = self.evaluate_expr(right)?;
            return Ok(Value::Boolean(right_value.is_truthy()));
        }

        let right_value = self.evaluate_expr(right)?;
        match operator {
            "==" => Ok(Value::Boolean(values_equal(&left_value, &right_value))),
            "!=" => Ok(Value::Boolean(!values_equal(&left_value, &right_value))),
            "<" | "<=" | ">" | ">=" => {
                let ordering = compare_values(&left_value, &right_value);
                Ok(Value::Boolean(match operator {
                    "<" => ordering == std::cmp::Ordering::Less,
                    "<=" => ordering != std::cmp::Ordering::Greater,
                    ">" => ordering == std::cmp::Ordering::Greater,
                    ">=" => ordering != std::cmp::Ordering::Less,
                    _ => unreachable!(),
                }))
            }
            _ => Err(QError::runtime(format!(
                "Opérateur logique '{operator}' inconnu"
            ))),
        }
    }
}

fn is_logical_operator(operator: &str) -> bool {
    matches!(
        operator,
        "et" | "ou" | "==" | "!=" | "<" | "<=" | ">" | ">="
    )
}

fn evaluate_algebraic_binary(operator: &str, left: &Value, right: &Value) -> Result<Value, QError> {
    let left_is_string = matches!(left, Value::String(_));
    let right_is_string = matches!(right, Value::String(_));

    if (left_is_string || right_is_string) && operator != "+" {
        return Err(QError::runtime(format!(
            "Opérateur '{operator}' invalide entre '{}' et '{}'",
            left.type_name(),
            right.type_name()
        )));
    }

    if left_is_string || right_is_string {
        return Ok(Value::String(format!(
            "{}{}",
            value_to_js_string(left),
            value_to_js_string(right)
        )));
    }

    let l = to_number(left);
    let r = to_number(right);

    Ok(match operator {
        "+" => Value::Number(l + r),
        "-" => Value::Number(l - r),
        "*" => Value::Number(l * r),
        "/" => Value::Number(l / r),
        "%" => Value::Number(l % r),
        _ => return Err(QError::runtime(format!("Opérateur '{operator}' inconnu"))),
    })
}

fn to_number(value: &Value) -> f64 {
    match value {
        Value::Number(n) => *n,
        Value::Boolean(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
        Value::Null => 0.0,
        _ => f64::NAN,
    }
}

fn to_index(value: &Value) -> Result<usize, QError> {
    match value {
        Value::Number(n) if *n >= 0.0 && n.fract() == 0.0 => Ok(*n as usize),
        Value::Number(n) => Err(QError::runtime(format!("Index de tableau invalide: '{n}'"))),
        other => Err(QError::runtime(format!(
            "Index de tableau invalide, attendu un nombre, reçu '{}'",
            other.type_name()
        ))),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Array(x), Value::Array(y)) => Rc::ptr_eq(x, y),
        (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
        (Value::Struct(x), Value::Struct(y)) => Rc::ptr_eq(x, y),
        (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    if let (Value::String(x), Value::String(y)) = (a, b) {
        return x.cmp(y);
    }
    to_number(a)
        .partial_cmp(&to_number(b))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn format_js_number(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// String conversion used by `ecrire`: matches qlang's own vocabulary
/// (`rien` for null, bracketed arrays, `<fonction #name>` for callables).
fn display_value(value: &Value) -> String {
    match value {
        Value::Break => "break".to_string(),
        Value::Continue => "continue".to_string(),
        Value::Null => "rien".to_string(),
        Value::Number(n) => format_js_number(*n),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let parts: Vec<String> = items.borrow().iter().map(display_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Function(f) => format!("<fonction #{}>", f.name()),
        Value::Struct(def) => format!("<structure {}>", def.name),
        Value::Instance(instance) => format!("<instance {}>", instance.struct_def.name),
        Value::Return(inner) => display_value(inner),
    }
}

/// String conversion used when a non-string operand is coerced by the `+`
/// operator: matches plain JS `ToString` semantics rather than qlang's
/// `ecrire` vocabulary (`null` instead of `rien`, comma-joined arrays).
fn value_to_js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Number(n) => format_js_number(*n),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let parts: Vec<String> = items.borrow().iter().map(value_to_js_string).collect();
            parts.join(",")
        }
        Value::Function(f) => format!("<fonction #{}>", f.name()),
        Value::Struct(def) => format!("<structure {}>", def.name),
        Value::Instance(instance) => format!("<instance {}>", instance.struct_def.name),
        Value::Break => "break".to_string(),
        Value::Continue => "continue".to_string(),
        Value::Return(inner) => value_to_js_string(inner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::make_global_env;
    use crate::module::{MapModuleResolver, NoopInput};

    fn make_interpreter() -> Interpreter {
        Interpreter::new(
            make_global_env(),
            Std::new(),
            Std::new(),
            Box::new(NoopInput),
            Box::new(MapModuleResolver::new()),
        )
    }

    fn run(interpreter: &mut Interpreter, input: &str) -> Result<Value, QError> {
        let mut lexer = Lexer::new();
        lexer.tokenize(input)?;
        let mut parser = Parser::new();
        parser.set_tokens(lexer.tokens().to_vec(), input);
        let program = parser.make_ast()?;
        interpreter.evaluate(&program)
    }

    #[test]
    fn evaluate_simple_numeric_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "40 + 2").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_priority_in_numeric_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "40 + 2 * 2").unwrap(),
            Value::Number(44.0)
        );
    }

    #[test]
    fn evaluate_parenthesis_in_numeric_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "(40 + 2) * 2").unwrap(),
            Value::Number(84.0)
        );
    }

    #[test]
    fn evaluate_boolean_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(run(&mut interpreter, "vrai").unwrap(), Value::Boolean(true));
        assert_eq!(
            run(&mut interpreter, "faux").unwrap(),
            Value::Boolean(false)
        );
    }

    #[test]
    fn evaluate_loose_operations() {
        let cases = [
            ("40 + 2", Value::Number(42.0)),
            ("vrai + vrai", Value::Number(2.0)),
            ("rien + 2", Value::Number(2.0)),
            ("40 + \"2\"", Value::String("402".to_string())),
            ("40 + vrai", Value::Number(41.0)),
            ("40 + rien", Value::Number(40.0)),
            ("40 + faux", Value::Number(40.0)),
        ];

        for (input, expected) in cases {
            let mut interpreter = make_interpreter();
            assert_eq!(
                run(&mut interpreter, input).unwrap(),
                expected,
                "input: {input}"
            );
        }
    }

    #[test]
    fn evaluate_simple_variable_declaration() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = 42").unwrap(),
            Value::Number(42.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("a").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_variable_assignment() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = 40\na = 2").unwrap(),
            Value::Number(2.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("a").unwrap(),
            Value::Number(2.0)
        );
    }

    #[test]
    fn evaluate_multiple_variable_assignment() {
        let mut interpreter = make_interpreter();
        let result = run(&mut interpreter, "dec a\ndec b\ndec c\na = b = c = 42").unwrap();
        assert_eq!(result, Value::Number(42.0));
        assert_eq!(
            interpreter.environment().lookup_variable("a").unwrap(),
            Value::Number(42.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("b").unwrap(),
            Value::Number(42.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("c").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_constant_declaration() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "constante a = 42").unwrap(),
            Value::Number(42.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("a").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_assigning_to_a_constant_is_an_error() {
        let mut interpreter = make_interpreter();
        let err = run(&mut interpreter, "constante a = 42\na = 1").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: Impossible de modifier la constante 'a'"
        );
    }

    #[test]
    fn evaluate_compound_assignment() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = 10\na += 5\na").unwrap(),
            Value::Number(15.0)
        );
        assert_eq!(
            run(&mut interpreter, "dec b = 10\nb -= 5\nb").unwrap(),
            Value::Number(5.0)
        );
    }

    #[test]
    fn evaluate_match_statement_selects_matching_case() {
        let mut interpreter = make_interpreter();
        let code = [
            "dec a = 2",
            "selon a",
            "cas 1 alors",
            "  ecrire \"un\"",
            "cas 2 alors",
            "  ecrire \"deux\"",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["deux"]);
    }

    #[test]
    fn evaluate_match_statement_falls_back_to_default() {
        let mut interpreter = make_interpreter();
        let code = [
            "selon 99",
            "cas 1 alors",
            "  ecrire \"un\"",
            "sinon",
            "  ecrire \"autre\"",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["autre"]);
    }

    #[test]
    fn evaluate_match_statement_without_match_and_without_default_returns_null() {
        let mut interpreter = make_interpreter();
        let code = ["selon 99", "cas 1 alors", "  ecrire \"un\"", "fin"].join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Null);
        assert!(interpreter.stdout().log().is_empty());
    }

    #[test]
    fn evaluate_variable_not_found() {
        let mut interpreter = make_interpreter();
        let err = run(&mut interpreter, "a").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: Variable 'a' non déclarée"
        );
    }

    #[test]
    fn evaluate_print_statement() {
        let mut interpreter = make_interpreter();
        assert_eq!(run(&mut interpreter, "ecrire 42").unwrap(), Value::Null);
        assert_eq!(interpreter.stdout().log(), ["42"]);
    }

    #[test]
    fn evaluate_unary_minus_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = -42\na = -a").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_unary_not_expression() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = non vrai").unwrap(),
            Value::Boolean(false)
        );
    }

    #[test]
    fn evaluate_if_else_if_else_statement() {
        let mut interpreter = make_interpreter();
        run(
            &mut interpreter,
            "si faux alors\n  ecrire 42\nsinonsi faux alors\n  ecrire 24\nsinon\n  ecrire 12\nfin",
        )
        .unwrap();
        assert_eq!(interpreter.stdout().log(), ["12"]);
    }

    #[test]
    fn evaluate_and_right_side_short_circuit() {
        let mut interpreter = make_interpreter();
        let code = [
            "dec age = 15",
            "dec isEvaluate = faux",
            "si age >= 18 et (isEvaluate = vrai) alors",
            "    ecrire \"Vous êtes majeur\"",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert!(interpreter.stdout().log().is_empty());
        assert_eq!(
            interpreter
                .environment()
                .lookup_variable("isEvaluate")
                .unwrap(),
            Value::Boolean(false)
        );
    }

    #[test]
    fn evaluate_or_right_side_short_circuit() {
        let mut interpreter = make_interpreter();
        let code = [
            "dec age = 20",
            "dec isEvaluate = faux",
            "si age >= 18 ou (isEvaluate = vrai) alors",
            "    ecrire \"Vous êtes majeur\"",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["Vous êtes majeur"]);
        assert_eq!(
            interpreter
                .environment()
                .lookup_variable("isEvaluate")
                .unwrap(),
            Value::Boolean(false)
        );
    }

    #[test]
    fn evaluate_declaration_in_if_statement_is_scoped() {
        let mut interpreter = make_interpreter();
        let code = ["si vrai alors", "    dec a = 42", "fin", "a"].join("\n");
        let err = run(&mut interpreter, &code).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: Variable 'a' non déclarée"
        );
    }

    #[test]
    fn evaluate_while_statement() {
        let mut interpreter = make_interpreter();
        let code = [
            "dec i = 0",
            "tantque i < 3 alors",
            "    ecrire i",
            "    i = i + 1",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["0", "1", "2"]);
    }

    #[test]
    fn evaluate_for_statement() {
        let mut interpreter = make_interpreter();
        let code = ["pour i de 0 jusque 3 alors", "    ecrire i", "fin"].join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["0", "1", "2", "3"]);
    }

    #[test]
    fn evaluate_for_statement_with_decrement_step() {
        let mut interpreter = make_interpreter();
        let code = [
            "pour i de 10 jusque i >= 0 evol -1 alors",
            "    ecrire i",
            "fin",
        ]
        .join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(
            interpreter.stdout().log(),
            ["10", "9", "8", "7", "6", "5", "4", "3", "2", "1", "0"]
        );
    }

    #[test]
    fn evaluate_array_created_returns_array_value() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "[1, 2, 3]").unwrap(),
            Value::array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn evaluate_array_access_on_complex_array() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "[[1, 2], [3, 4]][1][0]").unwrap(),
            Value::Number(3.0)
        );
    }

    #[test]
    fn evaluate_assignment_to_array_access() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = [1, 2, 3]\na[0] = 42").unwrap(),
            Value::Number(42.0)
        );
    }

    #[test]
    fn evaluate_push_in_array() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "dec a = []\na[] = 42\na[] = 24").unwrap(),
            Value::Number(24.0)
        );
        assert_eq!(
            interpreter.environment().lookup_variable("a").unwrap(),
            Value::array(vec![Value::Number(42.0), Value::Number(24.0)])
        );
    }

    #[test]
    fn evaluate_taille_builtin() {
        let mut interpreter = make_interpreter();
        assert_eq!(
            run(&mut interpreter, "taille([1, 2, 3])").unwrap(),
            Value::Number(3.0)
        );
    }

    #[test]
    fn evaluate_function_call() {
        let mut interpreter = make_interpreter();
        let code = ["fonction test()", "    retour 42", "fin", "test()"].join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_function_call_with_arguments() {
        let mut interpreter = make_interpreter();
        let code = [
            "fonction test(a, b)",
            "    retour a + b",
            "fin",
            "test(40, 2)",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_function_call_without_retour_returns_null() {
        let mut interpreter = make_interpreter();
        let code = ["fonction test()", "    ecrire 42", "fin", "test()"].join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Null);
        assert_eq!(interpreter.stdout().log(), ["42"]);
    }

    #[test]
    fn evaluate_fibonacci_function() {
        let mut interpreter = make_interpreter();
        let code = [
            "fonction fibonacci(n)",
            "    si n <= 1 alors",
            "        retour n",
            "    fin",
            "    retour fibonacci(n - 1) + fibonacci(n - 2)",
            "fin",
            "fibonacci(10)",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(55.0));
    }

    #[test]
    fn evaluate_function_returning_another_function() {
        let mut interpreter = make_interpreter();
        let code = [
            "fonction a(val)",
            "    fonction b(mul)",
            "        retour val * mul",
            "    fin",
            "    retour b",
            "fin",
            "a(42)(2)",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(84.0));
    }

    #[test]
    fn evaluate_call_with_closure_argument() {
        let mut interpreter = make_interpreter();
        let code = [
            "fonction a(val)",
            "    val(21, 2)",
            "fin",
            "a(fonction(a, b)",
            "    retour a * b",
            "fin)",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_include_merges_module_declarations() {
        let interpreter_env = make_global_env();
        let mut interpreter = Interpreter::new(
            interpreter_env,
            Std::new(),
            Std::new(),
            Box::new(NoopInput),
            Box::new(
                MapModuleResolver::new()
                    .with_module("math.q", "fonction double(x)\n    retour x * 2\nfin"),
            ),
        );

        let code = ["inclure \"math.q\"", "double(21)"].join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_include_is_idempotent() {
        let mut interpreter = Interpreter::new(
            make_global_env(),
            Std::new(),
            Std::new(),
            Box::new(NoopInput),
            Box::new(MapModuleResolver::new().with_module("greet.q", "ecrire \"salut\"")),
        );

        let code = ["inclure \"greet.q\"", "inclure \"greet.q\""].join("\n");
        run(&mut interpreter, &code).unwrap();
        assert_eq!(interpreter.stdout().log(), ["salut"]);
    }

    #[test]
    fn evaluate_struct_raw_instantiation_defaults_fields_to_null() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  publique champ",
            "fin",
            "dec p = Nom()",
            "p.champ",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Null);
    }

    #[test]
    fn evaluate_struct_field_default_value() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  publique champ = 42",
            "fin",
            "Nom().champ",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_struct_field_default_value_is_fresh_per_instance() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  publique valeurs = []",
            "fin",
            "dec a = Nom()",
            "dec b = Nom()",
            "a.valeurs[] = 1",
            "taille(b.valeurs)",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(0.0));
    }

    #[test]
    fn evaluate_struct_field_default_value_can_be_overridden() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  publique champ = 42",
            "fin",
            "dec p = Nom()",
            "p.champ = 1",
            "p.champ",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(1.0));
    }

    #[test]
    fn evaluate_struct_field_assignment_and_access() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  publique champ",
            "fin",
            "dec p = Nom()",
            "p.champ = 42",
            "p.champ",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_static_method_as_constructor() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Personne avec",
            "  publique nom",
            "fin",
            "dans Personne implemente",
            "  publique nouveau(nom)",
            "    dec p = Personne()",
            "    p.nom = nom",
            "    retour p",
            "  fin",
            "fin",
            "Personne.nouveau(\"Quentin\").nom",
        ]
        .join("\n");
        assert_eq!(
            run(&mut interpreter, &code).unwrap(),
            Value::String("Quentin".to_string())
        );
    }

    #[test]
    fn evaluate_instance_method_reads_moi() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Personne avec",
            "  publique nom",
            "fin",
            "dans Personne implemente",
            "  publique saluer(moi)",
            "    retour \"Bonjour \" + moi.nom",
            "  fin",
            "fin",
            "dec p = Personne()",
            "p.nom = \"Quentin\"",
            "p.saluer()",
        ]
        .join("\n");
        assert_eq!(
            run(&mut interpreter, &code).unwrap(),
            Value::String("Bonjour Quentin".to_string())
        );
    }

    #[test]
    fn evaluate_hidden_field_access_from_outside_is_an_error() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  cacher secret",
            "fin",
            "dec p = Nom()",
            "p.secret",
        ]
        .join("\n");
        let err = run(&mut interpreter, &code).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: 'secret' n'est pas accessible depuis l'extérieur de 'Nom'"
        );
    }

    #[test]
    fn evaluate_hidden_field_access_from_own_method_is_allowed() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  cacher secret",
            "fin",
            "dans Nom implemente",
            "  publique ecrire_secret(moi, valeur)",
            "    moi.secret = valeur",
            "  fin",
            "  publique lire_secret(moi)",
            "    retour moi.secret",
            "  fin",
            "fin",
            "dec p = Nom()",
            "p.ecrire_secret(42)",
            "p.lire_secret()",
        ]
        .join("\n");
        assert_eq!(run(&mut interpreter, &code).unwrap(), Value::Number(42.0));
    }

    #[test]
    fn evaluate_hidden_field_assignment_from_outside_is_an_error() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  cacher secret",
            "fin",
            "dec p = Nom()",
            "p.secret = 42",
        ]
        .join("\n");
        let err = run(&mut interpreter, &code).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: 'secret' n'est pas accessible depuis l'extérieur de 'Nom'"
        );
    }

    #[test]
    fn evaluate_calling_static_method_as_instance_method_is_an_error() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "fin",
            "dans Nom implemente",
            "  publique creer()",
            "    retour Nom()",
            "  fin",
            "fin",
            "dec p = Nom()",
            "p.creer()",
        ]
        .join("\n");
        let err = run(&mut interpreter, &code).unwrap_err();
        assert!(err.to_string().contains("méthode statique"));
    }

    #[test]
    fn evaluate_method_name_colliding_with_field_name_is_an_error() {
        let mut interpreter = make_interpreter();
        let code = [
            "structure Nom avec",
            "  cacher nom",
            "fin",
            "dans Nom implemente",
            "  publique nom(moi)",
            "    retour moi.nom",
            "  fin",
            "fin",
        ]
        .join("\n");
        let err = run(&mut interpreter, &code).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Erreur d'exécution: 'nom' est déjà utilisé comme champ de 'Nom', choisissez un autre nom pour la méthode"
        );
    }

    #[test]
    fn evaluate_include_missing_module_is_an_error() {
        let mut interpreter = make_interpreter();
        let err = run(&mut interpreter, "inclure \"absent.q\"").unwrap_err();
        assert!(matches!(err, QError::Module { .. }));
    }
}
