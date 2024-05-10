import { expect, test, describe, beforeEach } from "bun:test"

import Parser from "../parser"
import Lexer from "../lexer"
import Interpreter from "./interpreter"
import type { RuntimeValue } from "./values"

describe("Interpreter", () => {

    let lexer: Lexer
    let parser: Parser
    let interpreter: Interpreter

    const makeASTFromInput = (input: string): any => {
        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens)
        return parser.makeAST()
    }

    beforeEach(() => {
        lexer = new Lexer()
        parser = new Parser()
        interpreter = new Interpreter()
    })

    test('evaluate simple numeric expression', () => {
        const ast = makeASTFromInput('40 + 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 42 } as RuntimeValue)
    })

    test('evaluate priority in numeric expression', () => {
        const ast = makeASTFromInput('40 + 2 * 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 44 } as RuntimeValue)
    })

    test('evaluate parenthesis in numeric expression', () => {
        const ast = makeASTFromInput('(40 + 2) * 2')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 84 } as RuntimeValue)
    })

})
