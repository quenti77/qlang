import { expect, test, describe, beforeEach } from "bun:test"

import Parser from "./parser"
import Lexer from "./lexer"
import type { BinaryExpression, Identifier, NullLiteral, NumericLiteral } from "./ast"
import { OPERATORS } from "./token"

describe("Parser", () => {
    let lexer: Lexer
    let parser: Parser

    const makeASTFromInput = (input: string): any => {
        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens)
        return parser.makeAST()
    }

    beforeEach(() => {
        lexer = new Lexer()
        parser = new Parser()
    })

    test("make AST identifier", () => {
        const ast = makeASTFromInput('abc')
        const identifier: Identifier = { kind: 'Identifier', name: 'abc' }

        expect(ast).toEqual({
            kind: 'Program',
            body: [identifier]
        })
    })

    test("make AST numeric literal", () => {
        const ast = makeASTFromInput('42')
        const numericLiteral: NumericLiteral = { kind: 'NumericLiteral', value: 42 }

        expect(ast).toEqual({
            kind: 'Program',
            body: [numericLiteral]
        })
    })

    test("make AST simple null expression", () => {
        const ast = makeASTFromInput('rien')
        const nullLiteral: NullLiteral = { kind: 'NullLiteral', value: 'null' }

        expect(ast).toEqual({
            kind: 'Program',
            body: [nullLiteral]
        })
    })

    test.each(OPERATORS)("make AST simple binary expression for %s", (operator) => {
        const ast = makeASTFromInput(`40 ${operator} 2`)
        const leftExpr: NumericLiteral = { kind: 'NumericLiteral', value: 40 }
        const rightExpr: NumericLiteral = { kind: 'NumericLiteral', value: 2 }
        const binaryExpr = {
            kind: 'BinaryExpression',
            left: leftExpr,
            right: rightExpr,
            operator
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [binaryExpr]
        })
    })

    test("make priority parenthesis expression", () => {
        const ast = makeASTFromInput('5 * (2 + 3)')
        const leftExpr: NumericLiteral = { kind: 'NumericLiteral', value: 5 }
        const rightExpr: BinaryExpression = {
            kind: 'BinaryExpression',
            left: { kind: 'NumericLiteral', value: 2 } as NumericLiteral,
            right: { kind: 'NumericLiteral', value: 3 } as NumericLiteral,
            operator: '+'
        }
        const binaryExpr = {
            kind: 'BinaryExpression',
            left: leftExpr,
            right: rightExpr,
            operator: '*'
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [binaryExpr]
        })
    })
})
