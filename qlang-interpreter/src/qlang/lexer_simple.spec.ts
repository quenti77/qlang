import { expect, test, describe, beforeEach } from "bun:test"

import Lexer from "./lexer"
import { TokenType, createTokenAt, KEYWORDS, OPERATORS } from "./token"

describe("Simple Lexer", () => {

    let lexer: Lexer

    beforeEach(() => {
        lexer = new Lexer()
    })

    test('tokenize float numbers', () => {
        lexer.tokenize('1.2')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Number, '1.2', 1, 1, 1),
            createTokenAt(TokenType.EOF, '', 4, 1, 4),
        ])
    })

    test("tokenize an simple math expression", () => {
        const input = '40 + 20 * 60 - 40 / 30'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Number, '40', 1, 1, 1),
            createTokenAt(TokenType.BinaryOperator, '+', 4, 1, 4),
            createTokenAt(TokenType.Number, '20', 6, 1, 6),
            createTokenAt(TokenType.BinaryOperator, '*', 9, 1, 9),
            createTokenAt(TokenType.Number, '60', 11, 1, 11),
            createTokenAt(TokenType.BinaryOperator, '-', 14, 1, 14),
            createTokenAt(TokenType.Number, '40', 16, 1, 16),
            createTokenAt(TokenType.BinaryOperator, '/', 19, 1, 19),
            createTokenAt(TokenType.Number, '30', 21, 1, 21),
            createTokenAt(TokenType.EOF, '', 23, 1, 23),
        ])
    })

    test("tokenize a parenthesis expression", () => {
        const input = '(40 + 20)'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.OpenParenthesis, '(', 1, 1, 1),
            createTokenAt(TokenType.Number, '40', 2, 1, 2),
            createTokenAt(TokenType.BinaryOperator, '+', 5, 1, 5),
            createTokenAt(TokenType.Number, '20', 7, 1, 7),
            createTokenAt(TokenType.CloseParenthesis, ')', 9, 1, 9),
            createTokenAt(TokenType.EOF, '', 10, 1, 10),
        ])
    })

    test("tokenize a simple affectation", () => {
        const input = 'abc_123 = 41 + 23'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Identifier, 'abc_123', 1, 1, 1),
            createTokenAt(TokenType.Equals, '=', 9, 1, 9),
            createTokenAt(TokenType.Number, '41', 11, 1, 11),
            createTokenAt(TokenType.BinaryOperator, '+', 14, 1, 14),
            createTokenAt(TokenType.Number, '23', 16, 1, 16),
            createTokenAt(TokenType.EOF, '', 18, 1, 18),
        ])
    })

    test("tokenize a simple affectation with an underscore variable", () => {
        const input = '_abc = 41 + 23'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Identifier, '_abc', 1, 1, 1),
            createTokenAt(TokenType.Equals, '=', 6, 1, 6),
            createTokenAt(TokenType.Number, '41', 8, 1, 8),
            createTokenAt(TokenType.BinaryOperator, '+', 11, 1, 11),
            createTokenAt(TokenType.Number, '23', 13, 1, 13),
            createTokenAt(TokenType.EOF, '', 15, 1, 15),
        ])
    })

    test("tokenize a multiline input", () => {
        const lines = [
            'a = 1 + 2',
            'b = 3 * 4',
            'c = a + b'
        ]
        lexer.tokenize(lines.join('\n'))

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Identifier, 'a', 1, 1, 1),
            createTokenAt(TokenType.Equals, '=', 3, 1, 3),
            createTokenAt(TokenType.Number, '1', 5, 1, 5),
            createTokenAt(TokenType.BinaryOperator, '+', 7, 1, 7),
            createTokenAt(TokenType.Number, '2', 9, 1, 9),
            createTokenAt(TokenType.Identifier, 'b', 11, 2, 1),
            createTokenAt(TokenType.Equals, '=', 13, 2, 3),
            createTokenAt(TokenType.Number, '3', 15, 2, 5),
            createTokenAt(TokenType.BinaryOperator, '*', 17, 2, 7),
            createTokenAt(TokenType.Number, '4', 19, 2, 9),
            createTokenAt(TokenType.Identifier, 'c', 21, 3, 1),
            createTokenAt(TokenType.Equals, '=', 23, 3, 3),
            createTokenAt(TokenType.Identifier, 'a', 25, 3, 5),
            createTokenAt(TokenType.BinaryOperator, '+', 27, 3, 7),
            createTokenAt(TokenType.Identifier, 'b', 29, 3, 9),
            createTokenAt(TokenType.EOF, '', 30, 3, 10),
        ])
    })

    const keywords = Object.keys(KEYWORDS)
    test.each(keywords)("tokenize a keyword: %s", (keyword) => {
        const keywordLength = keyword.length
        lexer.tokenize(keyword + ' + a + 2')

        expect(lexer.Tokens).toEqual([
            createTokenAt(KEYWORDS[keyword], keyword, 1, 1, 1),
            createTokenAt(TokenType.BinaryOperator, '+', keywordLength + 2, 1, keywordLength + 2),
            createTokenAt(TokenType.Identifier, 'a', keywordLength + 4, 1, keywordLength + 4),
            createTokenAt(TokenType.BinaryOperator, '+', keywordLength + 6, 1, keywordLength + 6),
            createTokenAt(TokenType.Number, '2', keywordLength + 8, 1, keywordLength + 8),
            createTokenAt(TokenType.EOF, '', keywordLength + 9, 1, keywordLength + 9),
        ])
    })

    test.each(OPERATORS)("tokenize an operator: %s", (operator) => {
        lexer.tokenize(`a ${operator} 2`)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Identifier, 'a', 1, 1, 1),
            createTokenAt(TokenType.BinaryOperator, operator, 3, 1, 3),
            createTokenAt(TokenType.Number, '2', operator.length + 4, 1, operator.length + 4),
            createTokenAt(TokenType.EOF, '', operator.length + 5, 1, operator.length + 5),
        ])
    })

    test("tokenize a string", () => {
        lexer.tokenize('"hello world"')

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.String, 'hello world', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 14, 1, 14),
        ])
    })

    test("tokenize a string with escaped quotes", () => {
        lexer.tokenize(`"hello \\"w\\"orld"`)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.String, 'hello "w"orld', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 16, 1, 16),
        ])
    })

    test("tokenize a string with escaped backslashes", () => {
        lexer.tokenize(`"hello \\\\world"`)

        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.String, 'hello \\world', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 15, 1, 15),
        ])
    })

    test("tokenize a string with escaped newlines", () => {
        lexer.tokenize(`"hello\nworld"`)
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.String, 'hello\nworld', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 15, 2, 7),
        ])
    })

    test("tokenize a comment line", () => {
        lexer.tokenize('rem this is a comment')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.EOF, '', 22, 1, 22),
        ])
    })

    test("tokenize end of line comments", () => {
        lexer.tokenize('a = 1 rem this is a comment')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Identifier, 'a', 1, 1, 1),
            createTokenAt(TokenType.Equals, '=', 3, 1, 3),
            createTokenAt(TokenType.Number, '1', 5, 1, 5),
            createTokenAt(TokenType.EOF, '', 28, 1, 28),
        ])
    })

    test("tokenize unary operator", () => {
        lexer.tokenize('-1')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.UnaryOperator, '-', 1, 1, 1),
            createTokenAt(TokenType.Number, '1', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 3, 1, 3),
        ])
    })

    test("tokenize unary operator with parenthesis", () => {
        lexer.tokenize('-(1 + 2)')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.UnaryOperator, '-', 1, 1, 1),
            createTokenAt(TokenType.OpenParenthesis, '(', 2, 1, 2),
            createTokenAt(TokenType.Number, '1', 3, 1, 3),
            createTokenAt(TokenType.BinaryOperator, '+', 5, 1, 5),
            createTokenAt(TokenType.Number, '2', 7, 1, 7),
            createTokenAt(TokenType.CloseParenthesis, ')', 8, 1, 8),
            createTokenAt(TokenType.EOF, '', 9, 1, 9),
        ])
    })

    test("tokenize unary operator with binary operator", () => {
        lexer.tokenize('1 --a')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.Number, '1', 1, 1, 1),
            createTokenAt(TokenType.BinaryOperator, '-', 3, 1, 3),
            createTokenAt(TokenType.UnaryOperator, '-', 4, 1, 4),
            createTokenAt(TokenType.Identifier, 'a', 5, 1, 5),
            createTokenAt(TokenType.EOF, '', 6, 1, 6),
        ])
    })

    test("tokenize open and close brackets", () => {
        lexer.tokenize('[]')
        expect(lexer.Tokens).toEqual([
            createTokenAt(TokenType.OpenBrackets, '[', 1, 1, 1),
            createTokenAt(TokenType.CloseBrackets, ']', 2, 1, 2),
            createTokenAt(TokenType.EOF, '', 3, 1, 3),
        ])
    })
})
