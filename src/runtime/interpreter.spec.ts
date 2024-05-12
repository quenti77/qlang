import { expect, test, describe, beforeEach } from "bun:test"

import Parser from "../parser"
import Lexer from "../lexer"
import Interpreter from "./interpreter"
import Environment from "./environment"
import type { BooleanValue, NumberValue } from "./values"

describe("Interpreter", () => {

    let lexer: Lexer
    let parser: Parser
    let interpreter: Interpreter
    let env: Environment

    const makeASTFromInput = (input: string): any => {
        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens)
        return parser.makeAST()
    }

    beforeEach(() => {
        lexer = new Lexer()
        parser = new Parser()
        env = new Environment()
        interpreter = new Interpreter(env)
    })

    test('evaluate simple numeric expression', () => {
        const ast = makeASTFromInput('40 + 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

    test('evaluate priority in numeric expression', () => {
        const ast = makeASTFromInput('40 + 2 * 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 44 } as NumberValue)
    })

    test('evaluate parenthesis in numeric expression', () => {
        const ast = makeASTFromInput('(40 + 2) * 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 84 } as NumberValue)
    })

    test.each(['vrai', 'faux'])('evaluate boolean expression', (value) => {
        const ast = makeASTFromInput(value)
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'boolean', value: value === 'vrai' } as BooleanValue)
    })

    test('evaluate simple variable declaration', () => {
        const ast = makeASTFromInput('dec a = 42')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 42 } as NumberValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

    test('evaluate variable declaration with expression', () => {
        const ast = makeASTFromInput('dec a = 40 + 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 42 } as NumberValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

    test('evaluate variable assignment', () => {
        const ast = makeASTFromInput('dec a = 40\na = 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 2 } as NumberValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 2 } as NumberValue)
    })

    test('evaluate multiple variable assignment', () => {
        const ast = makeASTFromInput('dec a\ndec b\ndec c\na = b = c = 42')
        const result = interpreter.evaluate(ast)

        const expected = { type: 'number', value: 42 } as NumberValue
        expect(result).toEqual({ type: 'number', value: 42 } as NumberValue)
        expect(env.lookupVariable('a')).toEqual(expected)
        expect(env.lookupVariable('b')).toEqual(expected)
        expect(env.lookupVariable('c')).toEqual(expected)
    })

    test('evaluate variable not found', () => {
        const ast = makeASTFromInput('a')
        expect(() => interpreter.evaluate(ast)).toThrowError("Variable 'a' not declared")
    })
})