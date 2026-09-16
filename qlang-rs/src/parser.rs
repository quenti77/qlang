use std::collections::VecDeque;

use crate::ast::{
    Expr, FunctionDecl, MatchArm, MethodDecl, Program, Stmt, StructField, Visibility, SELF_PARAM,
};
use crate::error::QError;
use crate::position::Position;
use crate::token::{find_keywords_from_token, Token, TokenType};

pub struct Parser {
    code: String,
    tokens: VecDeque<Token>,
    previous_token: Option<Token>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            code: String::new(),
            tokens: VecDeque::new(),
            previous_token: None,
        }
    }

    pub fn set_tokens(&mut self, tokens: Vec<Token>, code: impl Into<String>) {
        self.code = code.into();
        self.tokens = tokens.into();
        self.previous_token = None;
    }

    pub fn make_ast(&mut self) -> Result<Program, QError> {
        let mut program = Vec::new();

        while !self.is_eof() {
            program.push(self.parse_statement()?);
        }

        Ok(program)
    }

    // Parser methods ordered by precedence.
    fn parse_statement(&mut self) -> Result<Stmt, QError> {
        match self.at().token_type {
            TokenType::Let => self.parse_variable_declaration_statement(false),
            TokenType::Const => self.parse_variable_declaration_statement(true),
            TokenType::Match => self.parse_match_statement(),
            TokenType::Print => self.parse_print_statement(),
            TokenType::If => self.parse_if_statement(true),
            TokenType::While => self.parse_while_statement(),
            TokenType::For => self.parse_for_statement(),
            TokenType::Function => Ok(Stmt::Function(self.parse_function_declaration()?)),
            TokenType::Break => {
                self.eat();
                Ok(Stmt::Break)
            }
            TokenType::Continue => {
                self.eat();
                Ok(Stmt::Continue)
            }
            TokenType::Return => self.parse_return_statement(),
            TokenType::Include => self.parse_include_statement(),
            TokenType::Structure => self.parse_struct_statement(),
            TokenType::In => self.parse_impl_statement(),
            _ => Ok(Stmt::Expr(self.parse_expression()?)),
        }
    }

    fn parse_variable_declaration_statement(&mut self, is_const: bool) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let identifier = self.eat_exactly(TokenType::Identifier, None)?.value;

        if self.at().token_type == TokenType::Equals {
            self.eat();
            let value = if self.at().token_type == TokenType::Function {
                Expr::Function(self.parse_function_declaration()?)
            } else {
                self.parse_expression()?
            };
            return Ok(Stmt::VariableDeclaration {
                identifier,
                value: Some(value),
                is_const,
            });
        }

        if is_const {
            return Err(QError::invalid_syntax(
                pos_start,
                self.at().position.clone(),
                format!("'constante {identifier}' doit être initialisée avec une valeur"),
                self.code.clone(),
            ));
        }

        Ok(Stmt::VariableDeclaration {
            identifier,
            value: None,
            is_const: false,
        })
    }

    fn parse_match_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let subject = self.parse_expression()?;

        let mut arms = Vec::new();
        while self.at().token_type == TokenType::Case {
            self.eat();
            let pattern = self.parse_expression()?;
            self.eat_exactly(TokenType::Then, Some(pos_start.clone()))?;
            let body = self.parse_block_statement(&[TokenType::Case, TokenType::Else])?;
            arms.push(MatchArm { pattern, body });
        }

        let mut default = None;
        if self.at().token_type == TokenType::Else {
            self.eat();
            default = Some(Box::new(self.parse_block_statement(&[])?));
        }

        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(Stmt::Match {
            subject,
            arms,
            default,
        })
    }

    fn parse_print_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        Ok(Stmt::Print(self.parse_expression()?))
    }

    fn parse_include_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        Ok(Stmt::Include(self.parse_expression()?))
    }

    fn parse_visibility(&mut self) -> Result<Visibility, QError> {
        let token = self.attempt(&[TokenType::Public, TokenType::Hidden, TokenType::Shared])?;
        self.eat();
        Ok(match token.token_type {
            TokenType::Public => Visibility::Public,
            TokenType::Hidden => Visibility::Hidden,
            TokenType::Shared => Visibility::Shared,
            _ => unreachable!(),
        })
    }

    fn parse_struct_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let name = self
            .eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
            .value;
        self.eat_exactly(TokenType::With, Some(pos_start.clone()))?;

        let mut fields = Vec::new();
        while self.at().token_type != TokenType::End {
            let visibility = self.parse_visibility()?;
            let field_name = self
                .eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
                .value;

            let default = if self.at().token_type == TokenType::Equals {
                self.eat();
                Some(self.parse_expression()?)
            } else {
                None
            };

            fields.push(StructField {
                visibility,
                name: field_name,
                default,
            });
        }
        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(Stmt::Struct { name, fields })
    }

    fn parse_impl_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let name = self
            .eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
            .value;
        self.eat_exactly(TokenType::Implements, Some(pos_start.clone()))?;

        let mut methods = Vec::new();
        while self.at().token_type != TokenType::End {
            methods.push(self.parse_method_declaration(&pos_start)?);
        }
        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(Stmt::Impl { name, methods })
    }

    fn parse_method_declaration(&mut self, pos_start: &Position) -> Result<MethodDecl, QError> {
        let visibility = self.parse_visibility()?;

        let identifier = self
            .eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
            .value;
        self.eat_exactly(TokenType::OpenParenthesis, Some(pos_start.clone()))?;

        let mut parameters = Vec::new();
        while self.at().token_type != TokenType::CloseParenthesis {
            parameters.push(
                self.eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
                    .value,
            );

            let token = self.attempt(&[TokenType::Comma, TokenType::CloseParenthesis])?;
            if token.token_type == TokenType::Comma {
                self.eat();
            }
        }
        self.eat();

        // A method is an instance method when its first parameter is the
        // explicit receiver `moi` (Python/Go/Rust-style explicit self,
        // rather than an implicit binding + a dedicated `statique` keyword).
        let is_static = parameters.first().map(String::as_str) != Some(SELF_PARAM);
        if !is_static {
            parameters.remove(0);
        }

        let body = match self.parse_block_statement(&[])? {
            Stmt::Block(body) => body,
            _ => unreachable!(),
        };
        self.eat_exactly(TokenType::End, Some(pos_start.clone()))?;

        Ok(MethodDecl {
            visibility,
            is_static,
            function: FunctionDecl {
                identifier: Some(identifier),
                parameters,
                body,
            },
        })
    }

    fn parse_if_statement(&mut self, end_needed: bool) -> Result<Stmt, QError> {
        if end_needed {
            self.eat_exactly(TokenType::If, None)?;
        }
        let mut pos_start = self.previous().position.clone();

        let condition = self.parse_expression()?;
        self.eat_exactly(TokenType::Then, Some(pos_start.clone()))?;

        pos_start = self.previous().position.clone();
        let then_branch =
            Box::new(self.parse_block_statement(&[TokenType::Else, TokenType::ElseIf])?);

        let mut else_branch = None;
        if self.at().token_type == TokenType::Else {
            self.eat();
            else_branch = Some(Box::new(self.parse_block_statement(&[])?));
        } else if self.at().token_type == TokenType::ElseIf {
            self.eat();
            else_branch = Some(Box::new(self.parse_if_statement(false)?));
        }

        if end_needed {
            self.eat_exactly(TokenType::End, Some(pos_start))?;
        }

        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let condition = self.parse_expression()?;
        self.eat_exactly(TokenType::Then, Some(pos_start.clone()))?;

        let body = Box::new(self.parse_block_statement(&[])?);
        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(Stmt::While { condition, body })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let identifier = self
            .eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
            .value;

        self.eat_exactly(TokenType::From, Some(pos_start.clone()))?;
        let from = self.parse_expression()?;

        self.eat_exactly(TokenType::Until, Some(pos_start.clone()))?;
        let mut until = self.parse_expression()?;
        if matches!(until, Expr::Numeric(_) | Expr::Identifier(_)) {
            until = Expr::Binary {
                left: Box::new(Expr::Identifier(identifier.clone())),
                right: Box::new(until),
                operator: "<=".to_string(),
            };
        }

        let mut step = Expr::Numeric(1.0);
        if self.at().token_type == TokenType::Step {
            self.eat();
            step = self.parse_expression()?;
        }

        let step = Expr::Assignment {
            target: Box::new(Expr::Identifier(identifier.clone())),
            value: Box::new(Expr::Binary {
                left: Box::new(Expr::Identifier(identifier.clone())),
                right: Box::new(step),
                operator: "+".to_string(),
            }),
        };

        self.eat_exactly(TokenType::Then, Some(pos_start.clone()))?;
        let body = Box::new(self.parse_block_statement(&[])?);
        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(Stmt::For {
            identifier,
            from,
            until,
            step,
            body,
        })
    }

    fn parse_function_declaration(&mut self) -> Result<FunctionDecl, QError> {
        self.eat();
        let pos_start = self.previous().position.clone();
        let identifier = if self.at().token_type == TokenType::OpenParenthesis {
            None
        } else {
            Some(
                self.eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
                    .value,
            )
        };

        self.eat_exactly(TokenType::OpenParenthesis, Some(pos_start.clone()))?;

        let mut parameters = Vec::new();
        while self.at().token_type != TokenType::CloseParenthesis {
            if parameters.len() >= 48 {
                return Err(QError::maximum_argument(
                    pos_start.clone(),
                    self.at().position.clone(),
                    format!(
                        "La fonction '{}' ne peut pas avoir plus de 48 arguments",
                        identifier.as_deref().unwrap_or("anonyme")
                    ),
                    self.code.clone(),
                ));
            }
            parameters.push(
                self.eat_exactly(TokenType::Identifier, Some(pos_start.clone()))?
                    .value,
            );

            let token = self.attempt(&[TokenType::Comma, TokenType::CloseParenthesis])?;
            if token.token_type == TokenType::Comma {
                self.eat();
            }
        }

        self.eat();
        let body = match self.parse_block_statement(&[])? {
            Stmt::Block(body) => body,
            _ => unreachable!(),
        };
        self.eat_exactly(TokenType::End, Some(pos_start))?;

        Ok(FunctionDecl {
            identifier,
            parameters,
            body,
        })
    }

    fn parse_block_statement(&mut self, with_condition: &[TokenType]) -> Result<Stmt, QError> {
        let mut body = Vec::new();

        while !self.is_eof()
            && self.at().token_type != TokenType::End
            && !with_condition.contains(&self.at().token_type)
        {
            body.push(self.parse_statement()?);
        }

        Ok(Stmt::Block(body))
    }

    fn parse_return_statement(&mut self) -> Result<Stmt, QError> {
        self.eat();
        Ok(Stmt::Return(self.parse_expression()?))
    }

    fn parse_expression(&mut self) -> Result<Expr, QError> {
        self.parse_assignment_expression()
    }

    fn parse_assignment_expression(&mut self) -> Result<Expr, QError> {
        let left = self.parse_logical_expression()?;

        if self.at().token_type == TokenType::CompoundAssign {
            // `a += b` desugars to `a = a + b` at parse time; no interpreter
            // support needed. `value[..1]` is safe: the token is always
            // exactly the operator character followed by `=`.
            let operator = self.eat().value[..1].to_string();
            let value = self.parse_assignment_expression()?;
            return Ok(Expr::Assignment {
                target: Box::new(left.clone()),
                value: Box::new(Expr::Binary {
                    left: Box::new(left),
                    right: Box::new(value),
                    operator,
                }),
            });
        }

        if self.at().token_type != TokenType::Equals {
            return Ok(left);
        }

        self.eat();
        let value = self.parse_assignment_expression()?;
        Ok(Expr::Assignment {
            target: Box::new(left),
            value: Box::new(value),
        })
    }

    fn parse_logical_expression(&mut self) -> Result<Expr, QError> {
        let mut left = self.parse_equality_expression()?;

        while self.at().value == "et" || self.at().value == "ou" {
            let operator = self.eat().value;
            let right = self.parse_equality_expression()?;
            left = Expr::Binary {
                left: Box::new(left),
                right: Box::new(right),
                operator,
            };
        }

        Ok(left)
    }

    fn parse_equality_expression(&mut self) -> Result<Expr, QError> {
        let mut left = self.parse_relational_expression()?;

        while self.at().value == "==" || self.at().value == "!=" {
            let operator = self.eat().value;
            let right = self.parse_relational_expression()?;
            left = Expr::Binary {
                left: Box::new(left),
                right: Box::new(right),
                operator,
            };
        }

        Ok(left)
    }

    fn parse_relational_expression(&mut self) -> Result<Expr, QError> {
        let mut left = self.parse_additive_expression()?;

        while [">", "<", ">=", "<="].contains(&self.at().value.as_str()) {
            let operator = self.eat().value;
            let right = self.parse_additive_expression()?;
            left = Expr::Binary {
                left: Box::new(left),
                right: Box::new(right),
                operator,
            };
        }

        Ok(left)
    }

    fn parse_additive_expression(&mut self) -> Result<Expr, QError> {
        let mut left = self.parse_multiplicative_expression()?;

        while self.at().value == "+" || self.at().value == "-" {
            let operator = self.eat().value;
            let right = self.parse_multiplicative_expression()?;
            left = Expr::Binary {
                left: Box::new(left),
                right: Box::new(right),
                operator,
            };
        }

        Ok(left)
    }

    fn parse_multiplicative_expression(&mut self) -> Result<Expr, QError> {
        let mut left = self.parse_unary_expression()?;

        while ["*", "/", "%"].contains(&self.at().value.as_str()) {
            let operator = self.eat().value;
            let right = self.parse_unary_expression()?;
            left = Expr::Binary {
                left: Box::new(left),
                right: Box::new(right),
                operator,
            };
        }

        Ok(left)
    }

    fn parse_unary_expression(&mut self) -> Result<Expr, QError> {
        if self.at().token_type == TokenType::UnaryOperator {
            let operator = self.eat().value;
            let value = self.parse_unary_expression()?;
            return Ok(Expr::Unary {
                operator,
                value: Box::new(value),
            });
        }

        self.parse_array_access_expression()
    }

    fn parse_array_access_expression(&mut self) -> Result<Expr, QError> {
        let mut expression = self.parse_array_expression()?;

        loop {
            if self.at().token_type == TokenType::OpenBrackets {
                self.eat();
                if self.at().token_type == TokenType::CloseBrackets {
                    self.eat();
                    expression = Expr::Member {
                        object: Box::new(expression),
                        property: None,
                    };
                    continue;
                }
                let index = self.parse_expression()?;
                self.eat_exactly(
                    TokenType::CloseBrackets,
                    Some(self.previous().position.clone()),
                )?;

                expression = Expr::Member {
                    object: Box::new(expression),
                    property: Some(Box::new(index)),
                };
            } else if self.at().token_type == TokenType::Dot {
                self.eat();
                let pos_start = self.previous().position.clone();
                let name = self
                    .eat_exactly(TokenType::Identifier, Some(pos_start))?
                    .value;
                expression = Expr::Member {
                    object: Box::new(expression),
                    property: Some(Box::new(Expr::Str(name))),
                };

                if self.at().token_type == TokenType::OpenParenthesis {
                    self.eat();
                    let mut arguments = Vec::new();
                    while self.at().token_type != TokenType::CloseParenthesis {
                        let argument = if self.at().token_type == TokenType::Function {
                            Expr::Function(self.parse_function_declaration()?)
                        } else {
                            self.parse_expression()?
                        };
                        arguments.push(argument);

                        let token =
                            self.attempt(&[TokenType::Comma, TokenType::CloseParenthesis])?;
                        if token.token_type == TokenType::Comma {
                            self.eat();
                        }
                    }
                    self.eat();
                    expression = Expr::Call {
                        callee: Box::new(expression),
                        arguments,
                    };
                }
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn parse_array_expression(&mut self) -> Result<Expr, QError> {
        if self.at().token_type != TokenType::OpenBrackets {
            return self.parse_call_expression();
        }
        self.eat();

        let mut elements = Vec::new();
        while self.at().token_type != TokenType::CloseBrackets {
            elements.push(self.parse_expression()?);

            let token = self.attempt(&[TokenType::Comma, TokenType::CloseBrackets])?;
            if token.token_type == TokenType::Comma {
                self.eat();
            }
        }

        self.eat();
        Ok(Expr::Array(elements))
    }

    fn parse_call_expression(&mut self) -> Result<Expr, QError> {
        let mut expression = self.parse_read_expression()?;

        while self.at().token_type == TokenType::OpenParenthesis {
            self.eat();

            let mut arguments = Vec::new();
            while self.at().token_type != TokenType::CloseParenthesis {
                let argument = if self.at().token_type == TokenType::Function {
                    Expr::Function(self.parse_function_declaration()?)
                } else {
                    self.parse_expression()?
                };
                arguments.push(argument);

                let token = self.attempt(&[TokenType::Comma, TokenType::CloseParenthesis])?;
                if token.token_type == TokenType::Comma {
                    self.eat();
                }
            }

            self.eat();
            expression = Expr::Call {
                callee: Box::new(expression),
                arguments,
            };
        }

        Ok(expression)
    }

    fn parse_read_expression(&mut self) -> Result<Expr, QError> {
        if self.at().token_type != TokenType::Read {
            return self.parse_primary_expression();
        }

        self.eat_exactly(TokenType::Read, None)?;
        Ok(Expr::Read(Box::new(self.parse_expression()?)))
    }

    fn parse_primary_expression(&mut self) -> Result<Expr, QError> {
        match self.at().token_type {
            TokenType::Identifier => Ok(Expr::Identifier(self.eat().value)),
            TokenType::Null => {
                self.eat();
                Ok(Expr::Null)
            }
            TokenType::Boolean => Ok(Expr::Boolean(self.eat().value == "vrai")),
            TokenType::Number => {
                let value = self.eat().value.parse::<f64>().unwrap_or(f64::NAN);
                Ok(Expr::Numeric(value))
            }
            TokenType::String => Ok(Expr::Str(self.eat().value)),
            TokenType::OpenParenthesis => {
                self.eat();
                let expression = self.parse_expression()?;
                self.eat_exactly(
                    TokenType::CloseParenthesis,
                    Some(self.previous().position.clone()),
                )?;
                Ok(expression)
            }
            _ => {
                let pos_start = self.previous().position.clone();
                let pos_end = self.at().position.clone();
                Err(QError::invalid_syntax(
                    pos_start,
                    pos_end,
                    format!("'{}' non attendu, expression attendue", self.at().value),
                    self.code.clone(),
                ))
            }
        }
    }

    // Utility methods.
    fn is_eof(&self) -> bool {
        self.at().token_type == TokenType::EOF
    }

    fn at(&self) -> Token {
        self.tokens.front().cloned().unwrap_or_else(|| Token {
            token_type: TokenType::EOF,
            value: String::new(),
            position: Position::new(0, 0, 0, ""),
        })
    }

    fn previous(&self) -> Token {
        self.previous_token.clone().unwrap_or_else(|| Token {
            token_type: TokenType::EOF,
            value: String::new(),
            position: Position::new(0, 0, 0, ""),
        })
    }

    fn eat(&mut self) -> Token {
        let token = self.tokens.pop_front().unwrap_or_else(|| Token {
            token_type: TokenType::EOF,
            value: String::new(),
            position: Position::new(0, 0, 0, ""),
        });
        self.previous_token = Some(token.clone());
        token
    }

    fn eat_exactly(
        &mut self,
        token_type: TokenType,
        pos_start: Option<Position>,
    ) -> Result<Token, QError> {
        let token = self.eat();

        if token.token_type != token_type {
            let pos_start = pos_start.unwrap_or_else(|| token.position.clone());
            let pos_end = token.position.clone();
            let tokens_needed = find_keywords_from_token(token_type).join(" ou ");

            if token.token_type == TokenType::EOF {
                let mut pos_start_eof = pos_start.clone();
                pos_start_eof.content = "<EOF>".to_string();
                return Err(QError::invalid_syntax(
                    pos_end,
                    pos_start_eof,
                    format!("Fin de fichier inattendue, attendu: '{tokens_needed}'"),
                    self.code.clone(),
                ));
            }

            return Err(QError::invalid_syntax(
                pos_start,
                pos_end.clone(),
                format!(
                    "'{}' non attendu, attendu: '{tokens_needed}'",
                    pos_end.content
                ),
                self.code.clone(),
            ));
        }

        Ok(token)
    }

    fn attempt(&self, types: &[TokenType]) -> Result<Token, QError> {
        let token = self.at();

        if types.contains(&token.token_type) {
            return Ok(token);
        }

        let types_string = types
            .iter()
            .map(|t| format!("'{t:?}'"))
            .collect::<Vec<_>>()
            .join(" ou ");
        Err(QError::invalid_syntax(
            token.position.clone(),
            token.position.clone(),
            format!("'{}' non attendu, attendu: {types_string}", token.value),
            self.code.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::token::OPERATORS;

    fn make_ast(input: &str) -> Program {
        let mut lexer = Lexer::new();
        lexer.tokenize(input).expect("tokenize should succeed");
        let mut parser = Parser::new();
        parser.set_tokens(lexer.tokens().to_vec(), input);
        parser.make_ast().expect("parse should succeed")
    }

    fn try_make_ast(input: &str) -> Result<Program, QError> {
        let mut lexer = Lexer::new();
        lexer.tokenize(input).expect("tokenize should succeed");
        let mut parser = Parser::new();
        parser.set_tokens(lexer.tokens().to_vec(), input);
        parser.make_ast()
    }

    #[test]
    fn ast_identifier() {
        assert_eq!(
            make_ast("abc"),
            vec![Stmt::Expr(Expr::Identifier("abc".to_string()))]
        );
    }

    #[test]
    fn ast_numeric_literal() {
        assert_eq!(make_ast("42"), vec![Stmt::Expr(Expr::Numeric(42.0))]);
    }

    #[test]
    fn ast_float_numeric_literal() {
        assert_eq!(make_ast("42.42"), vec![Stmt::Expr(Expr::Numeric(42.42))]);
    }

    #[test]
    fn ast_null_expression() {
        assert_eq!(make_ast("rien"), vec![Stmt::Expr(Expr::Null)]);
    }

    #[test]
    fn ast_binary_expression_for_every_operator() {
        for operator in OPERATORS {
            let ast = make_ast(&format!("40 {operator} 2"));
            assert_eq!(
                ast,
                vec![Stmt::Expr(Expr::Binary {
                    left: Box::new(Expr::Numeric(40.0)),
                    right: Box::new(Expr::Numeric(2.0)),
                    operator: operator.to_string(),
                })]
            );
        }
    }

    #[test]
    fn ast_priority_parenthesis_expression() {
        let ast = make_ast("5 * (2 + 3)");
        assert_eq!(
            ast,
            vec![Stmt::Expr(Expr::Binary {
                left: Box::new(Expr::Numeric(5.0)),
                right: Box::new(Expr::Binary {
                    left: Box::new(Expr::Numeric(2.0)),
                    right: Box::new(Expr::Numeric(3.0)),
                    operator: "+".to_string(),
                }),
                operator: "*".to_string(),
            })]
        );
    }

    #[test]
    fn ast_boolean_literal() {
        assert_eq!(make_ast("vrai"), vec![Stmt::Expr(Expr::Boolean(true))]);
        assert_eq!(make_ast("faux"), vec![Stmt::Expr(Expr::Boolean(false))]);
    }

    #[test]
    fn ast_variable_declaration() {
        assert_eq!(
            make_ast("dec abc = 42"),
            vec![Stmt::VariableDeclaration {
                identifier: "abc".to_string(),
                value: Some(Expr::Numeric(42.0)),
                is_const: false,
            }]
        );
    }

    #[test]
    fn ast_variable_declaration_multiline_expression() {
        assert_eq!(
            make_ast("dec abc =\n40 + 2"),
            vec![Stmt::VariableDeclaration {
                identifier: "abc".to_string(),
                value: Some(Expr::Binary {
                    left: Box::new(Expr::Numeric(40.0)),
                    right: Box::new(Expr::Numeric(2.0)),
                    operator: "+".to_string(),
                }),
                is_const: false,
            }]
        );
    }

    #[test]
    fn ast_variable_declaration_without_value() {
        assert_eq!(
            make_ast("dec abc"),
            vec![Stmt::VariableDeclaration {
                identifier: "abc".to_string(),
                value: None,
                is_const: false,
            }]
        );
    }

    #[test]
    fn ast_variable_assignment() {
        assert_eq!(
            make_ast("dec abc = 42\nabc = 2"),
            vec![
                Stmt::VariableDeclaration {
                    identifier: "abc".to_string(),
                    value: Some(Expr::Numeric(42.0)),
                    is_const: false,
                },
                Stmt::Expr(Expr::Assignment {
                    target: Box::new(Expr::Identifier("abc".to_string())),
                    value: Box::new(Expr::Numeric(2.0)),
                }),
            ]
        );
    }

    #[test]
    fn ast_constant_declaration() {
        assert_eq!(
            make_ast("constante abc = 42"),
            vec![Stmt::VariableDeclaration {
                identifier: "abc".to_string(),
                value: Some(Expr::Numeric(42.0)),
                is_const: true,
            }]
        );
    }

    #[test]
    fn ast_constant_declaration_without_value_is_an_error() {
        assert!(try_make_ast("constante abc").is_err());
    }

    #[test]
    fn ast_compound_assignment_is_desugared() {
        for (operator, expected) in [("+=", "+"), ("-=", "-")] {
            let ast = make_ast(&format!("abc {operator} 2"));
            assert_eq!(
                ast,
                vec![Stmt::Expr(Expr::Assignment {
                    target: Box::new(Expr::Identifier("abc".to_string())),
                    value: Box::new(Expr::Binary {
                        left: Box::new(Expr::Identifier("abc".to_string())),
                        right: Box::new(Expr::Numeric(2.0)),
                        operator: expected.to_string(),
                    }),
                })],
                "operator {operator}"
            );
        }
    }

    #[test]
    fn ast_match_statement() {
        let code = [
            "selon abc",
            "cas 1 alors",
            "  ecrire \"un\"",
            "cas 2 alors",
            "  ecrire \"deux\"",
            "sinon",
            "  ecrire \"autre\"",
            "fin",
        ]
        .join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Match {
                subject: Expr::Identifier("abc".to_string()),
                arms: vec![
                    MatchArm {
                        pattern: Expr::Numeric(1.0),
                        body: Stmt::Block(vec![Stmt::Print(Expr::Str("un".to_string()))]),
                    },
                    MatchArm {
                        pattern: Expr::Numeric(2.0),
                        body: Stmt::Block(vec![Stmt::Print(Expr::Str("deux".to_string()))]),
                    },
                ],
                default: Some(Box::new(Stmt::Block(vec![Stmt::Print(Expr::Str(
                    "autre".to_string()
                ))]))),
            }]
        );
    }

    #[test]
    fn ast_match_statement_without_default() {
        let code = ["selon abc", "cas 1 alors", "  ecrire \"un\"", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Match {
                subject: Expr::Identifier("abc".to_string()),
                arms: vec![MatchArm {
                    pattern: Expr::Numeric(1.0),
                    body: Stmt::Block(vec![Stmt::Print(Expr::Str("un".to_string()))]),
                }],
                default: None,
            }]
        );
    }

    #[test]
    fn ast_string_literal() {
        assert_eq!(
            make_ast("\"hello\""),
            vec![Stmt::Expr(Expr::Str("hello".to_string()))]
        );
    }

    #[test]
    fn ast_print_statement() {
        assert_eq!(
            make_ast("ecrire 42"),
            vec![Stmt::Print(Expr::Numeric(42.0))]
        );
    }

    #[test]
    fn ast_read_expression() {
        assert_eq!(
            make_ast("lire \"Nom :\""),
            vec![Stmt::Expr(Expr::Read(Box::new(Expr::Str(
                "Nom :".to_string()
            ))))]
        );
    }

    #[test]
    fn ast_include_statement() {
        assert_eq!(
            make_ast("inclure \"module.q\""),
            vec![Stmt::Include(Expr::Str("module.q".to_string()))]
        );
    }

    #[test]
    fn ast_if_statement_single_block() {
        let ast = make_ast("si 42 alors\n  ecrire 42\nfin");
        assert_eq!(
            ast,
            vec![Stmt::If {
                condition: Expr::Numeric(42.0),
                then_branch: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(42.0))])),
                else_branch: None,
            }]
        );
    }

    #[test]
    fn ast_if_else_statement() {
        let code = ["si 42 alors", "  ecrire 42", "sinon", "  ecrire 2", "fin"].join("\n");
        let ast = make_ast(&code);
        assert_eq!(
            ast,
            vec![Stmt::If {
                condition: Expr::Numeric(42.0),
                then_branch: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(42.0))])),
                else_branch: Some(Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(2.0))]))),
            }]
        );
    }

    #[test]
    fn ast_if_else_if_statement() {
        let code = [
            "si 42 alors",
            "  ecrire 42",
            "sinonsi 2 alors",
            "  ecrire 2",
            "fin",
        ]
        .join("\n");
        let ast = make_ast(&code);
        assert_eq!(
            ast,
            vec![Stmt::If {
                condition: Expr::Numeric(42.0),
                then_branch: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(42.0))])),
                else_branch: Some(Box::new(Stmt::If {
                    condition: Expr::Numeric(2.0),
                    then_branch: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(2.0))])),
                    else_branch: None,
                })),
            }]
        );
    }

    #[test]
    fn ast_while_statement() {
        let code = ["tantque vrai alors", "  ecrire 42", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::While {
                condition: Expr::Boolean(true),
                body: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Numeric(42.0))])),
            }]
        );
    }

    #[test]
    fn ast_for_statement() {
        let code = [
            "pour abc de 1 jusque 10 evol 2 alors",
            "  ecrire abc",
            "fin",
        ]
        .join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::For {
                identifier: "abc".to_string(),
                from: Expr::Numeric(1.0),
                until: Expr::Binary {
                    left: Box::new(Expr::Identifier("abc".to_string())),
                    right: Box::new(Expr::Numeric(10.0)),
                    operator: "<=".to_string(),
                },
                step: Expr::Assignment {
                    target: Box::new(Expr::Identifier("abc".to_string())),
                    value: Box::new(Expr::Binary {
                        left: Box::new(Expr::Identifier("abc".to_string())),
                        right: Box::new(Expr::Numeric(2.0)),
                        operator: "+".to_string(),
                    }),
                },
                body: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Identifier(
                    "abc".to_string()
                ))])),
            }]
        );
    }

    #[test]
    fn ast_for_statement_without_evol() {
        let code = ["pour abc de 1 jusque 10 alors", "  ecrire abc", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::For {
                identifier: "abc".to_string(),
                from: Expr::Numeric(1.0),
                until: Expr::Binary {
                    left: Box::new(Expr::Identifier("abc".to_string())),
                    right: Box::new(Expr::Numeric(10.0)),
                    operator: "<=".to_string(),
                },
                step: Expr::Assignment {
                    target: Box::new(Expr::Identifier("abc".to_string())),
                    value: Box::new(Expr::Binary {
                        left: Box::new(Expr::Identifier("abc".to_string())),
                        right: Box::new(Expr::Numeric(1.0)),
                        operator: "+".to_string(),
                    }),
                },
                body: Box::new(Stmt::Block(vec![Stmt::Print(Expr::Identifier(
                    "abc".to_string()
                ))])),
            }]
        );
    }

    #[test]
    fn ast_simple_array_expression() {
        assert_eq!(
            make_ast("[1, 2, 3]"),
            vec![Stmt::Expr(Expr::Array(vec![
                Expr::Numeric(1.0),
                Expr::Numeric(2.0),
                Expr::Numeric(3.0)
            ]))]
        );
    }

    #[test]
    fn ast_nested_array_expression() {
        assert_eq!(
            make_ast("[1, [2, 3,], 4,]"),
            vec![Stmt::Expr(Expr::Array(vec![
                Expr::Numeric(1.0),
                Expr::Array(vec![Expr::Numeric(2.0), Expr::Numeric(3.0)]),
                Expr::Numeric(4.0),
            ]))]
        );
    }

    #[test]
    fn ast_bad_array_expressions_error() {
        for code in ["[1, 2, 3", "[1, 2 3]", "[1, 2, 3[]"] {
            assert!(try_make_ast(code).is_err(), "expected error for {code}");
        }
    }

    #[test]
    fn ast_array_access_from_variable() {
        let code = ["dec abc = [1, 2, 3]", "ecrire abc[1]"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![
                Stmt::VariableDeclaration {
                    identifier: "abc".to_string(),
                    value: Some(Expr::Array(vec![
                        Expr::Numeric(1.0),
                        Expr::Numeric(2.0),
                        Expr::Numeric(3.0)
                    ])),
                    is_const: false,
                },
                Stmt::Print(Expr::Member {
                    object: Box::new(Expr::Identifier("abc".to_string())),
                    property: Some(Box::new(Expr::Numeric(1.0))),
                }),
            ]
        );
    }

    #[test]
    fn ast_push_new_element_into_array() {
        let code = ["dec tab = []", "tab[] = 1"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![
                Stmt::VariableDeclaration {
                    identifier: "tab".to_string(),
                    value: Some(Expr::Array(vec![])),
                    is_const: false,
                },
                Stmt::Expr(Expr::Assignment {
                    target: Box::new(Expr::Member {
                        object: Box::new(Expr::Identifier("tab".to_string())),
                        property: None
                    }),
                    value: Box::new(Expr::Numeric(1.0)),
                }),
            ]
        );
    }

    #[test]
    fn ast_call_function_with_empty_parameters() {
        assert_eq!(
            make_ast("abc()"),
            vec![Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Identifier("abc".to_string())),
                arguments: vec![]
            })]
        );
    }

    #[test]
    fn ast_call_function_with_multiple_parameters() {
        assert_eq!(
            make_ast("abc(42, \"hello\", a)"),
            vec![Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Identifier("abc".to_string())),
                arguments: vec![
                    Expr::Numeric(42.0),
                    Expr::Str("hello".to_string()),
                    Expr::Identifier("a".to_string()),
                ],
            })]
        );
    }

    #[test]
    fn ast_function_declaration_without_parameters() {
        let code = ["fonction abc()", "  ecrire 42", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Function(FunctionDecl {
                identifier: Some("abc".to_string()),
                parameters: vec![],
                body: vec![Stmt::Print(Expr::Numeric(42.0))],
            })]
        );
    }

    #[test]
    fn ast_function_declaration_with_parameters() {
        let code = ["fonction addition(a, b)", "    retour a + b", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Function(FunctionDecl {
                identifier: Some("addition".to_string()),
                parameters: vec!["a".to_string(), "b".to_string()],
                body: vec![Stmt::Return(Expr::Binary {
                    left: Box::new(Expr::Identifier("a".to_string())),
                    right: Box::new(Expr::Identifier("b".to_string())),
                    operator: "+".to_string(),
                })],
            })]
        );
    }

    #[test]
    fn ast_function_without_identifier() {
        let code = ["fonction (a)", "    ecrire a", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Function(FunctionDecl {
                identifier: None,
                parameters: vec!["a".to_string()],
                body: vec![Stmt::Print(Expr::Identifier("a".to_string()))],
            })]
        );
    }

    #[test]
    fn ast_struct_statement() {
        let code = [
            "structure Nom avec",
            "  publique champ1",
            "  cacher champ2",
            "fin",
        ]
        .join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Struct {
                name: "Nom".to_string(),
                fields: vec![
                    StructField {
                        visibility: Visibility::Public,
                        name: "champ1".to_string(),
                        default: None,
                    },
                    StructField {
                        visibility: Visibility::Hidden,
                        name: "champ2".to_string(),
                        default: None,
                    },
                ],
            }]
        );
    }

    #[test]
    fn ast_struct_field_with_default_value() {
        let code = ["structure Nom avec", "  publique champ = 42", "fin"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Struct {
                name: "Nom".to_string(),
                fields: vec![StructField {
                    visibility: Visibility::Public,
                    name: "champ".to_string(),
                    default: Some(Expr::Numeric(42.0)),
                }],
            }]
        );
    }

    #[test]
    fn ast_impl_statement() {
        let code = [
            "dans Nom implemente",
            "  publique nouveau()",
            "    retour Nom()",
            "  fin",
            "  publique saluer(moi)",
            "    ecrire moi",
            "  fin",
            "fin",
        ]
        .join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Impl {
                name: "Nom".to_string(),
                methods: vec![
                    MethodDecl {
                        visibility: Visibility::Public,
                        is_static: true,
                        function: FunctionDecl {
                            identifier: Some("nouveau".to_string()),
                            parameters: vec![],
                            body: vec![Stmt::Return(Expr::Call {
                                callee: Box::new(Expr::Identifier("Nom".to_string())),
                                arguments: vec![],
                            })],
                        },
                    },
                    MethodDecl {
                        visibility: Visibility::Public,
                        is_static: false,
                        function: FunctionDecl {
                            identifier: Some("saluer".to_string()),
                            parameters: vec![],
                            body: vec![Stmt::Print(Expr::Identifier("moi".to_string()))],
                        },
                    },
                ],
            }]
        );
    }

    #[test]
    fn ast_member_access_with_dot() {
        assert_eq!(
            make_ast("moi.nom"),
            vec![Stmt::Expr(Expr::Member {
                object: Box::new(Expr::Identifier("moi".to_string())),
                property: Some(Box::new(Expr::Str("nom".to_string()))),
            })]
        );
    }

    #[test]
    fn ast_method_call_with_dot() {
        assert_eq!(
            make_ast("Nom.nouveau(42)"),
            vec![Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Member {
                    object: Box::new(Expr::Identifier("Nom".to_string())),
                    property: Some(Box::new(Expr::Str("nouveau".to_string()))),
                }),
                arguments: vec![Expr::Numeric(42.0)],
            })]
        );
    }

    #[test]
    fn ast_call_with_anonymous_function() {
        let code = ["abc(fonction (a)", "    ecrire a", "fin)"].join("\n");
        assert_eq!(
            make_ast(&code),
            vec![Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Identifier("abc".to_string())),
                arguments: vec![Expr::Function(FunctionDecl {
                    identifier: None,
                    parameters: vec!["a".to_string()],
                    body: vec![Stmt::Print(Expr::Identifier("a".to_string()))],
                })],
            })]
        );
    }
}
