import { expect, test, describe, beforeEach } from "bun:test"

import Lexer from "./lexer"
import { TokenType, createToken, KEYWORDS, OPERATORS } from "./token"

describe("Simple Lexer", () => {

    let lexer: Lexer

    beforeEach(() => {
        lexer = new Lexer()
    })

    test("tokenize an simple math expression", () => {
        const input = '40 + 20 * 60 - 40 / 30'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.Number, '40', 1, 1),
            createToken(TokenType.BinaryOperator, '+', 1, 4),
            createToken(TokenType.Number, '20', 1, 6),
            createToken(TokenType.BinaryOperator, '*', 1, 9),
            createToken(TokenType.Number, '60', 1, 11),
            createToken(TokenType.BinaryOperator, '-', 1, 14),
            createToken(TokenType.Number, '40', 1, 16),
            createToken(TokenType.BinaryOperator, '/', 1, 19),
            createToken(TokenType.Number, '30', 1, 21),
            createToken(TokenType.EOF, '', 1, 23),
        ])
    })

    test("tokenize a parenthesis expression", () => {
        const input = '(40 + 20)'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.OpenParenthesis, '(', 1, 1),
            createToken(TokenType.Number, '40', 1, 2),
            createToken(TokenType.BinaryOperator, '+', 1, 5),
            createToken(TokenType.Number, '20', 1, 7),
            createToken(TokenType.CloseParenthesis, ')', 1, 9),
            createToken(TokenType.EOF, '', 1, 10),
        ])
    })

    test("tokenize a simple affectation", () => {
        const input = 'abc_123 = 41 + 23'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.Identifier, 'abc_123', 1, 1),
            createToken(TokenType.Equals, '=', 1, 9),
            createToken(TokenType.Number, '41', 1, 11),
            createToken(TokenType.BinaryOperator, '+', 1, 14),
            createToken(TokenType.Number, '23', 1, 16),
            createToken(TokenType.EOF, '', 1, 18),
        ])
    })

    test("tokenize a simple affectation with an underscore variable", () => {
        const input = '_abc = 41 + 23'
        lexer.tokenize(input)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.Identifier, '_abc', 1, 1),
            createToken(TokenType.Equals, '=', 1, 6),
            createToken(TokenType.Number, '41', 1, 8),
            createToken(TokenType.BinaryOperator, '+', 1, 11),
            createToken(TokenType.Number, '23', 1, 13),
            createToken(TokenType.EOF, '', 1, 15),
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
            createToken(TokenType.Identifier, 'a', 1, 1),
            createToken(TokenType.Equals, '=', 1, 3),
            createToken(TokenType.Number, '1', 1, 5),
            createToken(TokenType.BinaryOperator, '+', 1, 7),
            createToken(TokenType.Number, '2', 1, 9),
            createToken(TokenType.Identifier, 'b', 2, 1),
            createToken(TokenType.Equals, '=', 2, 3),
            createToken(TokenType.Number, '3', 2, 5),
            createToken(TokenType.BinaryOperator, '*', 2, 7),
            createToken(TokenType.Number, '4', 2, 9),
            createToken(TokenType.Identifier, 'c', 3, 1),
            createToken(TokenType.Equals, '=', 3, 3),
            createToken(TokenType.Identifier, 'a', 3, 5),
            createToken(TokenType.BinaryOperator, '+', 3, 7),
            createToken(TokenType.Identifier, 'b', 3, 9),
            createToken(TokenType.EOF, '', 3, 10),
        ])
    })

    const keywords = Object.keys(KEYWORDS)
    test.each(keywords)("tokenize a keyword: %s", (keyword) => {
        const keywordLength = keyword.length
        lexer.tokenize(keyword + ' + a + 2')

        expect(lexer.Tokens).toEqual([
            createToken(KEYWORDS[keyword], keyword, 1, 1),
            createToken(TokenType.BinaryOperator, '+', 1, keywordLength + 2),
            createToken(TokenType.Identifier, 'a', 1, keywordLength + 4),
            createToken(TokenType.BinaryOperator, '+', 1, keywordLength + 6),
            createToken(TokenType.Number, '2', 1, keywordLength + 8),
            createToken(TokenType.EOF, '', 1, keywordLength + 9),
        ])
    })

    test.each(OPERATORS)("tokenize an operator: %s", (operator) => {
        lexer.tokenize(`a ${operator} 2`)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.Identifier, 'a', 1, 1),
            createToken(TokenType.BinaryOperator, operator, 1, 3),
            createToken(TokenType.Number, '2', 1, operator.length + 4),
            createToken(TokenType.EOF, '', 1, operator.length + 5),
        ])
    })

    test("tokenize a string", () => {
        lexer.tokenize('"hello world"')

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.String, 'hello world', 1, 2),
            createToken(TokenType.EOF, '', 1, 14),
        ])
    })

    test("tokenize a string with escaped quotes", () => {
        lexer.tokenize(`"hello \\"w\\"orld"`)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.String, 'hello "w"orld', 1, 2),
            createToken(TokenType.EOF, '', 1, 16),
        ])
    })

    test("tokenize a string with escaped backslashes", () => {
        lexer.tokenize(`"hello \\\\world"`)

        expect(lexer.Tokens).toEqual([
            createToken(TokenType.String, 'hello \\world', 1, 2),
            createToken(TokenType.EOF, '', 1, 15),
        ])
    })

    test("tokenize a string with escaped newlines", () => {
        lexer.tokenize(`"hello\nworld"`)
        expect(lexer.Tokens).toEqual([
            createToken(TokenType.String, 'hello\nworld', 1, 2),
            createToken(TokenType.EOF, '', 2, 7),
        ])
    })

})
