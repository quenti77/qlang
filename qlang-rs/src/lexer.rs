use std::collections::VecDeque;

use crate::error::QError;
use crate::position::Position;
use crate::token::{create_token, lookup_keyword, Token, TokenType, OPERATORS};

pub struct Lexer {
    position: Position,
    lines: VecDeque<String>,
    tokens: Vec<Token>,
    src: VecDeque<char>,
    code: String,
    current_line: Option<String>,
}

impl Default for Lexer {
    fn default() -> Self {
        Self::new()
    }
}

impl Lexer {
    pub fn new() -> Self {
        Self {
            position: Position::new(0, 0, 0, ""),
            lines: VecDeque::new(),
            tokens: Vec::new(),
            src: VecDeque::new(),
            code: String::new(),
            current_line: None,
        }
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn tokenize(&mut self, input: &str) -> Result<(), QError> {
        self.code = input.to_string();
        self.reset();
        self.lines = input.split('\n').map(String::from).collect();

        while self.has_more_lines() {
            self.next_line();
            self.tokenize_line()?;
        }
        self.push_token(TokenType::EOF, String::new());
        Ok(())
    }

    fn tokenize_line(&mut self) -> Result<(), QError> {
        if self.current_line.is_none() {
            return Ok(());
        }

        while !self.src.is_empty() {
            if self.process_operator() {
                let ch = self.src.pop_front().unwrap();
                self.push_token(TokenType::BinaryOperator, ch.to_string());
            } else if self.front() == Some('-') {
                let ch = self.src.pop_front().unwrap();
                self.push_token(TokenType::UnaryOperator, ch.to_string());
            } else if self.front() == Some('(') {
                self.src.pop_front();
                self.push_token(TokenType::OpenParenthesis, "(");
            } else if self.front() == Some(')') {
                self.src.pop_front();
                self.push_token(TokenType::CloseParenthesis, ")");
            } else if self.front() == Some('[') {
                self.src.pop_front();
                self.push_token(TokenType::OpenBrackets, "[");
            } else if self.front() == Some(']') {
                self.src.pop_front();
                self.push_token(TokenType::CloseBrackets, "]");
            } else if self.front() == Some(',') {
                self.src.pop_front();
                self.push_token(TokenType::Comma, ",");
            } else if self.is_start_logical_operator(self.front()) {
                let mut current = self.src.pop_front().unwrap().to_string();
                if self.front() == Some('=') {
                    current.push(self.src.pop_front().unwrap());
                }
                let token_type = if current == "=" { TokenType::Equals } else { TokenType::BinaryOperator };
                self.push_token(token_type, current);
            } else if self.front() == Some('.') || self.is_number(self.front()) {
                self.process_number()?;
            } else if self.front() == Some('"') {
                self.process_string()?;
            } else if self.is_identifier(self.front(), false) {
                let mut token = self.src.pop_front().unwrap().to_string();
                while self.is_identifier(self.front(), true) {
                    token.push(self.src.pop_front().unwrap());
                }

                if let Some(reserved) = lookup_keyword(&token) {
                    self.push_token(reserved, token);
                } else if token == "rem" {
                    self.add_col(&token);
                    while !self.src.is_empty() {
                        self.eat();
                    }
                    break;
                } else {
                    self.push_token(TokenType::Identifier, token);
                }
            } else {
                let ch = self.src.pop_front().unwrap();
                self.position.advance(false, &ch.to_string());
            }
        }

        Ok(())
    }

    fn process_operator(&self) -> bool {
        let first = match self.front() {
            Some(c) => c,
            None => return false,
        };
        if self.is_start_logical_operator(Some(first)) {
            return false;
        }
        if !OPERATORS.contains(&first.to_string().as_str()) {
            return false;
        }

        if first == '-' {
            if let Some(next) = self.src.get(1).copied() {
                if next.is_ascii_alphanumeric() || next == '_' || next == '(' || next == ')' {
                    return false;
                }
            }
        }

        true
    }

    fn is_start_logical_operator(&self, ch: Option<char>) -> bool {
        matches!(ch, Some('=') | Some('!') | Some('<') | Some('>'))
    }

    fn process_number(&mut self) -> Result<(), QError> {
        let mut token = self.src.pop_front().unwrap().to_string();
        let mut has_dot = token == ".";

        while self.front().map(|c| c.is_ascii_digit() || c == '.').unwrap_or(false) {
            if self.front() == Some('.') {
                if has_dot {
                    let pos_start = self.position.clone();
                    self.add_col(&token);
                    let pos_end = self.position.clone();
                    return Err(QError::illegal_char(pos_start, pos_end, ".", self.code.clone()));
                }
                has_dot = true;
            }
            token.push(self.src.pop_front().unwrap());
        }

        self.push_token(TokenType::Number, token);
        Ok(())
    }

    fn process_string(&mut self) -> Result<(), QError> {
        self.eat();
        let pos_start = self.position.clone();
        let mut value = String::new();

        loop {
            if self.front() == Some('"') {
                break;
            }
            let current = self.eat();
            match current {
                Some('\\') => {
                    let next = self.src.pop_front();
                    match next {
                        Some('\\') => value.push('\\'),
                        Some('"') => value.push('"'),
                        Some('n') => self.next_line(),
                        other => {
                            self.add_col("\\");
                            let pos_end = self.position.clone();
                            let details = other.map(|c| c.to_string()).unwrap_or_default();
                            return Err(QError::illegal_char(pos_start, pos_end, details, self.code.clone()));
                        }
                    }
                }
                None => {
                    if !self.has_more_lines() {
                        let pos_end = self.position.clone();
                        return Err(QError::string_unterminated(pos_start, pos_end, self.code.clone()));
                    }
                    self.next_line();
                    value.push('\n');
                }
                Some(c) => value.push(c),
            }
        }

        let mut token_pos = pos_start;
        token_pos.content = value.clone();
        self.tokens.push(create_token(TokenType::String, value, &token_pos));
        self.eat();
        Ok(())
    }

    fn eat(&mut self) -> Option<char> {
        let current = self.src.pop_front();
        let s = current.map(|c| c.to_string()).unwrap_or_default();
        self.position.advance(false, &s);
        current
    }

    fn add_col(&mut self, text: &str) {
        self.position.advance(false, text);
    }

    fn push_token(&mut self, token_type: TokenType, value: impl Into<String>) {
        let value = value.into();
        let mut token_position = self.position.clone();
        token_position.content = value.clone();

        self.tokens.push(create_token(token_type, value.clone(), &token_position));
        self.position.advance(false, &value);
    }

    fn is_identifier(&self, ch: Option<char>, with_number: bool) -> bool {
        match ch {
            Some(c) if with_number => c.is_ascii_alphanumeric() || c == '_',
            Some(c) => c.is_ascii_alphabetic() || c == '_',
            None => false,
        }
    }

    fn is_number(&self, ch: Option<char>) -> bool {
        ch.map(|c| c.is_ascii_digit()).unwrap_or(false)
    }

    fn front(&self) -> Option<char> {
        self.src.front().copied()
    }

    fn has_more_lines(&self) -> bool {
        !self.lines.is_empty()
    }

    fn next_line(&mut self) {
        self.position.advance(true, "");
        self.current_line = self.lines.pop_front();
        self.src = self.current_line.clone().unwrap_or_default().chars().collect();
    }

    fn reset(&mut self) {
        self.position = Position::new(0, 0, 0, "");
        self.lines = VecDeque::new();
        self.tokens = Vec::new();
        self.current_line = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::create_token_at;

    fn tokenize(input: &str) -> Vec<Token> {
        let mut lexer = Lexer::new();
        lexer.tokenize(input).expect("tokenize should succeed");
        lexer.tokens().to_vec()
    }

    #[test]
    fn tokenize_float_numbers() {
        assert_eq!(
            tokenize("1.2"),
            vec![
                create_token_at(TokenType::Number, "1.2", 1, 1, 1),
                create_token_at(TokenType::EOF, "", 4, 1, 4),
            ]
        );
    }

    #[test]
    fn tokenize_simple_math_expression() {
        let input = "40 + 20 * 60 - 40 / 30";
        assert_eq!(
            tokenize(input),
            vec![
                create_token_at(TokenType::Number, "40", 1, 1, 1),
                create_token_at(TokenType::BinaryOperator, "+", 4, 1, 4),
                create_token_at(TokenType::Number, "20", 6, 1, 6),
                create_token_at(TokenType::BinaryOperator, "*", 9, 1, 9),
                create_token_at(TokenType::Number, "60", 11, 1, 11),
                create_token_at(TokenType::BinaryOperator, "-", 14, 1, 14),
                create_token_at(TokenType::Number, "40", 16, 1, 16),
                create_token_at(TokenType::BinaryOperator, "/", 19, 1, 19),
                create_token_at(TokenType::Number, "30", 21, 1, 21),
                create_token_at(TokenType::EOF, "", 23, 1, 23),
            ]
        );
    }

    #[test]
    fn tokenize_parenthesis_expression() {
        assert_eq!(
            tokenize("(40 + 20)"),
            vec![
                create_token_at(TokenType::OpenParenthesis, "(", 1, 1, 1),
                create_token_at(TokenType::Number, "40", 2, 1, 2),
                create_token_at(TokenType::BinaryOperator, "+", 5, 1, 5),
                create_token_at(TokenType::Number, "20", 7, 1, 7),
                create_token_at(TokenType::CloseParenthesis, ")", 9, 1, 9),
                create_token_at(TokenType::EOF, "", 10, 1, 10),
            ]
        );
    }

    #[test]
    fn tokenize_simple_affectation() {
        assert_eq!(
            tokenize("abc_123 = 41 + 23"),
            vec![
                create_token_at(TokenType::Identifier, "abc_123", 1, 1, 1),
                create_token_at(TokenType::Equals, "=", 9, 1, 9),
                create_token_at(TokenType::Number, "41", 11, 1, 11),
                create_token_at(TokenType::BinaryOperator, "+", 14, 1, 14),
                create_token_at(TokenType::Number, "23", 16, 1, 16),
                create_token_at(TokenType::EOF, "", 18, 1, 18),
            ]
        );
    }

    #[test]
    fn tokenize_affectation_with_underscore_variable() {
        assert_eq!(
            tokenize("_abc = 41 + 23"),
            vec![
                create_token_at(TokenType::Identifier, "_abc", 1, 1, 1),
                create_token_at(TokenType::Equals, "=", 6, 1, 6),
                create_token_at(TokenType::Number, "41", 8, 1, 8),
                create_token_at(TokenType::BinaryOperator, "+", 11, 1, 11),
                create_token_at(TokenType::Number, "23", 13, 1, 13),
                create_token_at(TokenType::EOF, "", 15, 1, 15),
            ]
        );
    }

    #[test]
    fn tokenize_multiline_input() {
        let lines = ["a = 1 + 2", "b = 3 * 4", "c = a + b"];
        assert_eq!(
            tokenize(&lines.join("\n")),
            vec![
                create_token_at(TokenType::Identifier, "a", 1, 1, 1),
                create_token_at(TokenType::Equals, "=", 3, 1, 3),
                create_token_at(TokenType::Number, "1", 5, 1, 5),
                create_token_at(TokenType::BinaryOperator, "+", 7, 1, 7),
                create_token_at(TokenType::Number, "2", 9, 1, 9),
                create_token_at(TokenType::Identifier, "b", 11, 2, 1),
                create_token_at(TokenType::Equals, "=", 13, 2, 3),
                create_token_at(TokenType::Number, "3", 15, 2, 5),
                create_token_at(TokenType::BinaryOperator, "*", 17, 2, 7),
                create_token_at(TokenType::Number, "4", 19, 2, 9),
                create_token_at(TokenType::Identifier, "c", 21, 3, 1),
                create_token_at(TokenType::Equals, "=", 23, 3, 3),
                create_token_at(TokenType::Identifier, "a", 25, 3, 5),
                create_token_at(TokenType::BinaryOperator, "+", 27, 3, 7),
                create_token_at(TokenType::Identifier, "b", 29, 3, 9),
                create_token_at(TokenType::EOF, "", 30, 3, 10),
            ]
        );
    }

    #[test]
    fn tokenize_every_keyword() {
        for (keyword, token_type) in crate::token::KEYWORDS {
            let keyword_len = keyword.chars().count();
            let input = format!("{keyword} + a + 2");
            assert_eq!(
                tokenize(&input),
                vec![
                    create_token_at(*token_type, *keyword, 1, 1, 1),
                    create_token_at(TokenType::BinaryOperator, "+", keyword_len + 2, 1, keyword_len + 2),
                    create_token_at(TokenType::Identifier, "a", keyword_len + 4, 1, keyword_len + 4),
                    create_token_at(TokenType::BinaryOperator, "+", keyword_len + 6, 1, keyword_len + 6),
                    create_token_at(TokenType::Number, "2", keyword_len + 8, 1, keyword_len + 8),
                    create_token_at(TokenType::EOF, "", keyword_len + 9, 1, keyword_len + 9),
                ],
                "keyword {keyword}"
            );
        }
    }

    #[test]
    fn tokenize_every_operator() {
        for operator in OPERATORS {
            let input = format!("a {operator} 2");
            let operator_len = operator.chars().count();
            assert_eq!(
                tokenize(&input),
                vec![
                    create_token_at(TokenType::Identifier, "a", 1, 1, 1),
                    create_token_at(TokenType::BinaryOperator, *operator, 3, 1, 3),
                    create_token_at(TokenType::Number, "2", operator_len + 4, 1, operator_len + 4),
                    create_token_at(TokenType::EOF, "", operator_len + 5, 1, operator_len + 5),
                ],
                "operator {operator}"
            );
        }
    }

    #[test]
    fn tokenize_string() {
        assert_eq!(
            tokenize("\"hello world\""),
            vec![
                create_token_at(TokenType::String, "hello world", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 14, 1, 14),
            ]
        );
    }

    #[test]
    fn tokenize_string_with_escaped_quotes() {
        assert_eq!(
            tokenize(r#""hello \"w\"orld""#),
            vec![
                create_token_at(TokenType::String, "hello \"w\"orld", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 16, 1, 16),
            ]
        );
    }

    #[test]
    fn tokenize_string_with_escaped_backslashes() {
        assert_eq!(
            tokenize(r#""hello \\world""#),
            vec![
                create_token_at(TokenType::String, "hello \\world", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 15, 1, 15),
            ]
        );
    }

    #[test]
    fn tokenize_string_with_escaped_newlines() {
        assert_eq!(
            tokenize("\"hello\nworld\""),
            vec![
                create_token_at(TokenType::String, "hello\nworld", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 15, 2, 7),
            ]
        );
    }

    #[test]
    fn tokenize_comment_line() {
        assert_eq!(
            tokenize("rem this is a comment"),
            vec![create_token_at(TokenType::EOF, "", 22, 1, 22)]
        );
    }

    #[test]
    fn tokenize_end_of_line_comment() {
        assert_eq!(
            tokenize("a = 1 rem this is a comment"),
            vec![
                create_token_at(TokenType::Identifier, "a", 1, 1, 1),
                create_token_at(TokenType::Equals, "=", 3, 1, 3),
                create_token_at(TokenType::Number, "1", 5, 1, 5),
                create_token_at(TokenType::EOF, "", 28, 1, 28),
            ]
        );
    }

    #[test]
    fn tokenize_unary_operator() {
        assert_eq!(
            tokenize("-1"),
            vec![
                create_token_at(TokenType::UnaryOperator, "-", 1, 1, 1),
                create_token_at(TokenType::Number, "1", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 3, 1, 3),
            ]
        );
    }

    #[test]
    fn tokenize_unary_operator_with_parenthesis() {
        assert_eq!(
            tokenize("-(1 + 2)"),
            vec![
                create_token_at(TokenType::UnaryOperator, "-", 1, 1, 1),
                create_token_at(TokenType::OpenParenthesis, "(", 2, 1, 2),
                create_token_at(TokenType::Number, "1", 3, 1, 3),
                create_token_at(TokenType::BinaryOperator, "+", 5, 1, 5),
                create_token_at(TokenType::Number, "2", 7, 1, 7),
                create_token_at(TokenType::CloseParenthesis, ")", 8, 1, 8),
                create_token_at(TokenType::EOF, "", 9, 1, 9),
            ]
        );
    }

    #[test]
    fn tokenize_unary_operator_with_binary_operator() {
        assert_eq!(
            tokenize("1 --a"),
            vec![
                create_token_at(TokenType::Number, "1", 1, 1, 1),
                create_token_at(TokenType::BinaryOperator, "-", 3, 1, 3),
                create_token_at(TokenType::UnaryOperator, "-", 4, 1, 4),
                create_token_at(TokenType::Identifier, "a", 5, 1, 5),
                create_token_at(TokenType::EOF, "", 6, 1, 6),
            ]
        );
    }

    #[test]
    fn tokenize_open_and_close_brackets() {
        assert_eq!(
            tokenize("[]"),
            vec![
                create_token_at(TokenType::OpenBrackets, "[", 1, 1, 1),
                create_token_at(TokenType::CloseBrackets, "]", 2, 1, 2),
                create_token_at(TokenType::EOF, "", 3, 1, 3),
            ]
        );
    }

    #[test]
    fn tokenize_include_keyword() {
        assert_eq!(
            tokenize("inclure \"module.q\""),
            vec![
                create_token_at(TokenType::Include, "inclure", 1, 1, 1),
                create_token_at(TokenType::String, "module.q", 10, 1, 10),
                create_token_at(TokenType::EOF, "", 19, 1, 19),
            ]
        );
    }

    #[test]
    fn tokenize_number_with_two_dots_is_an_error() {
        let mut lexer = Lexer::new();
        let err = lexer.tokenize("1.2.3").unwrap_err();
        assert!(matches!(err, QError::IllegalChar(_)));
    }

    #[test]
    fn tokenize_unterminated_string_is_an_error() {
        let mut lexer = Lexer::new();
        let err = lexer.tokenize("\"hello").unwrap_err();
        assert!(matches!(err, QError::StringUnterminated(_)));
    }
}
