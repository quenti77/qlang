import { expect, test, describe, beforeEach } from "bun:test"

import Parser from "../parser"
import Lexer from "../lexer"
import Interpreter from "./interpreter"
import Environment from "./environment"
import {
    MK_ARRAY,
    MK_BREAK,
    MK_CONTINUE,
    MK_NULL,
    MK_NUMBER,
    MK_RETURN,
    type BooleanValue,
    type NullValue,
    type NumberValue,
    type StringValue,
} from "./values"
import { Std } from "./std"
import { Program, WhileStatement } from "../ast"
import { makeGlobalEnv } from "@/infrastructure/monacoEditor/languages/qlangGlobals"

describe("Interpreter", () => {

    let lexer: Lexer
    let parser: Parser
    let interpreter: Interpreter
    let env: Environment
    let stdOut: Std
    let stdErr: Std

    const makeASTFromInput = (input: string): Program => {
        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens, input)
        return parser.makeAST()
    }

    beforeEach(() => {
        lexer = new Lexer()
        parser = new Parser()
        env = makeGlobalEnv()
        stdOut = new Std()
        stdErr = new Std()

        interpreter = new Interpreter(env, stdOut, stdErr)
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

    const operations = [
        { input: '40 + 2', expected: { type: 'number', value: 42 } as NumberValue },
        { input: 'vrai + vrai', expected: { type: 'number', value: 2 } as NumberValue },
        { input: 'rien + 2', expected: { type: 'number', value: 2 } as NumberValue },
        { input: '40 + "2"', expected: { type: 'string', value: '402' } as StringValue },
        { input: '40 + vrai', expected: { type: 'number', value: 41 } as NumberValue },
        { input: '40 + rien', expected: { type: 'number', value: 40 } as NumberValue },
        { input: '40 + faux', expected: { type: 'number', value: 40 } as NumberValue },
    ]
    test.each(operations)("evaluate operations with inputs %#", ({ input, expected }) => {
        const ast = makeASTFromInput(input)
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(expected)
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
        expect(() => interpreter.evaluate(ast)).toThrowError("Variable 'a' non déclarée")
    })

    test('evaluate variable assignment with string', () => {
        const ast = makeASTFromInput('dec a = "hello"')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'string', value: 'hello' } as StringValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'string', value: 'hello' } as StringValue)
    })

    test('evaluate print statement', () => {
        const ast = makeASTFromInput('ecrire 42')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['42'])
    })

    test('evaluate unary minus epxression', () => {
        const ast = makeASTFromInput('dec a = -42\na = -a')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'number', value: 42 } as NumberValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

    test('evaluate unary not epxression', () => {
        const ast = makeASTFromInput('dec a = non vrai')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'boolean', value: false } as BooleanValue)
        expect(env.lookupVariable('a')).toEqual({ type: 'boolean', value: false } as BooleanValue)
    })

    test('evaluate if statement', () => {
        const ast = makeASTFromInput('si vrai alors\n  ecrire 42\nfin')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['42'])
    })

    test('evaluate if else statement', () => {
        const ast = makeASTFromInput('si faux alors\n  ecrire 42\nsinon\n  ecrire 24\nfin')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['24'])
    })

    test('evaluate if else if statement', () => {
        const ast = makeASTFromInput('si faux alors\n  ecrire 42\nsinonsi vrai alors\n  ecrire 24\nfin')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['24'])
    })

    test('evaluate if else if else statement', () => {
        const ast = makeASTFromInput('si faux alors\n  ecrire 42\nsinonsi faux alors\n  ecrire 24\nsinon\n  ecrire 12\nfin')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['12'])
    })

    test('evaluate and right side short circuit', () => {
        const code = [
            'dec age = 15',
            'dec isEvaluate = faux',
            'si age >= 18 et (isEvaluate = vrai) alors',
            '    ecrire "Vous êtes majeur"',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual([])
        expect(env.lookupVariable('isEvaluate')).toEqual({ type: 'boolean', value: false } as BooleanValue)
    })

    test('evaluate or right side short circuit', () => {
        const code = [
            'dec age = 20',
            'dec isEvaluate = faux',
            'si age >= 18 ou (isEvaluate = vrai) alors',
            '    ecrire "Vous êtes majeur"',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['Vous êtes majeur'])
        expect(env.lookupVariable('isEvaluate')).toEqual({ type: 'boolean', value: false } as BooleanValue)
    })

    test('evaluate declation and assignment in if statement throw an error outside if', () => {
        const code = [
            'si vrai alors',
            '    dec a = 42',
            'fin',
            'a'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        expect(() => interpreter.evaluate(ast)).toThrowError("Variable 'a' non déclarée")
    })

    test('evaluate while statement', () => {
        const code = [
            'dec i = 0',
            'tantque i < 3 alors',
            '    ecrire i',
            '    i = i + 1',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['0', '1', '2'])
    })

    test('evaluate for statement', () => {
        const code = [
            'pour i de 0 jusque 3 alors',
            '    ecrire i',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['0', '1', '2', '3'])
    })

    test('evaluate for statement with decrement step', () => {
        const code = [
            'pour i de 10 jusque i >= 0 evol -1 alors',
            '    ecrire i',
            'fin',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual({ type: 'null', value: null } as NullValue)
        expect(stdOut.Log).toEqual(['10', '9', '8', '7', '6', '5', '4', '3', '2', '1', '0'])
    })

    const breakStatements = [
        { statement: 'arreter', expected: MK_BREAK() },
        { statement: 'continuer', expected: MK_CONTINUE() },
        { statement: 'retour 42', expected: MK_RETURN(MK_NUMBER(42)) },
    ]
    test.each(breakStatements)('evaluate with directly break statement in block return this', (el) => {
        const code = [
            'tantque vrai alors',
            `    ${el.statement}`,
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const whileStatement = ast.body[0] as WhileStatement
        const result = interpreter.evaluate(whileStatement.body)

        expect(result).toEqual(el.expected)
    })

    test.each(breakStatements)('evaluate break statement in if block return this', (el) => {
        const code = [
            'tantque vrai alors',
            '    si vrai alors',
            '        si vrai alors',
            `            ${el.statement}`,
            '        fin',
            '    fin',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const whileStatement = ast.body[0] as WhileStatement
        const result = interpreter.evaluate(whileStatement.body)

        expect(result).toEqual(el.expected)
    })

    test('evaluate array created returns array value', () => {
        const ast = makeASTFromInput('[1, 2, 3]')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_ARRAY([MK_NUMBER(1), MK_NUMBER(2), MK_NUMBER(3)]))
    })

    test('evaluate array access returns value at index', () => {
        const ast = makeASTFromInput('[1, 2, 3][0]')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(1))
    })

    test('evaluate array access on the complex array returns value at index', () => {
        const ast = makeASTFromInput('[[1, 2], [3, 4]][1][0]')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(3))
    })

    test('evaluate assignment to array access', () => {
        const ast = makeASTFromInput('dec a = [1, 2, 3]\na[0] = 42')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(42))
    })

    test('evaluate assigment to push in array', () => {
        const ast = makeASTFromInput('dec a = []\na[] = 42\na[] = 24')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(24))
        expect(env.lookupVariable('a')).toEqual(MK_ARRAY([MK_NUMBER(42), MK_NUMBER(24)]))
    })

    test('evaluate call to "taille" function returns array length', () => {
        const ast = makeASTFromInput('taille([1, 2, 3])')
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(3))
    })

    test('evaluate function declaration', () => {
        const code = [
            'fonction test()',
            '    retour 42',
            'fin',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toHaveProperty('type', 'function')
        expect(env.lookupVariable('test')).not.toBeNull()
    })

    test('evaluate function call', () => {
        const code = [
            'fonction test()',
            '    retour 42',
            'fin',
            'test()',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(42))
    })

    test('evaluate function call with arguments', () => {
        const code = [
            'fonction test(a, b)',
            '    retour a + b',
            'fin',
            'test(40, 2)',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(42))
    })

    test('evaluate function call without retour statement returns null', () => {
        const code = [
            'fonction test()',
            '    ecrire 42',
            'fin',
            'test()',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NULL())
        expect(stdOut.Log).toEqual(['42'])
    })

    test('evaluate fibonacci function', () => {
        const code = [
            'fonction fibonacci(n)',
            '    si n <= 1 alors',
            '        retour n',
            '    fin',
            '    retour fibonacci(n - 1) + fibonacci(n - 2)',
            'fin',
            'fibonacci(10)',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(55))
    })

    test('evaluate function returning another function', () => {
        const code = [
            'fonction a(val)',
            '    fonction b(mul)',
            '        retour val * mul',
            '    fin',
            '    retour b',
            'fin',
            'a(42)(2)',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(84))
    })

    test('evaluate declaration variable with anonymous function', () => {
        const code = [
            'dec a = fonction()',
            '    retour 42',
            'fin',
            'a()',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(42))
    })

    test('evaluate call fonction with closure argument', () => {
        const code = [
            'fonction a(val)',
            '    val(21, 2)',
            'fin',
            'a(fonction(a, b)',
            '    retour a * b',
            'fin)',
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const result = interpreter.evaluate(ast)

        expect(result).toEqual(MK_NUMBER(42))
    })
})
