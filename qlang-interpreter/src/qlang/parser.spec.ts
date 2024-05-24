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

    test.each(['vrai', 'faux'])("make AST boolean literal for %s", (value) => {
        const ast = makeASTFromInput(value)
        const booleanLiteral = { kind: 'BooleanLiteral', value: value === 'vrai' }

        expect(ast).toEqual({
            kind: 'Program',
            body: [booleanLiteral]
        })
    })

    test("make AST variable declaration", () => {
        const ast = makeASTFromInput('dec abc = 42')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'NumericLiteral', value: 42 } as NumericLiteral
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration]
        })
    })

    test("make AST variable declaration with expression multilines", () => {
        const ast = makeASTFromInput('dec abc =\n40 + 2')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: {
                kind: 'BinaryExpression',
                left: { kind: 'NumericLiteral', value: 40 } as NumericLiteral,
                right: { kind: 'NumericLiteral', value: 2 } as NumericLiteral,
                operator: '+'
            } as BinaryExpression
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration]
        })
    })

    test("make AST variable declaration without value", () => {
        const ast = makeASTFromInput('dec abc')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc'
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration]
        })
    })

    test("make AST variable assignment", () => {
        const ast = makeASTFromInput('dec abc = 42\nabc = 2')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'NumericLiteral', value: 42 } as NumericLiteral
        }
        const assignment = {
            kind: 'AssignmentExpression',
            assignment: { kind: 'Identifier', name: 'abc' } as Identifier,
            value: { kind: 'NumericLiteral', value: 2 } as NumericLiteral
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration, assignment]
        })
    })

    test("make AST variable assignment with multiple variables", () => {
        const ast = makeASTFromInput('dec abc = 42\ndec def = abc\nabc = def = 2')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'NumericLiteral', value: 42 } as NumericLiteral
        }
        const variableDeclaration2 = {
            kind: 'VariableDeclarationStatement',
            identifier: 'def',
            value: { kind: 'Identifier', name: 'abc' } as Identifier
        }
        const assignment = {
            kind: 'AssignmentExpression',
            assignment: { kind: 'Identifier', name: 'abc' } as Identifier,
            value: {
                kind: 'AssignmentExpression',
                assignment: { kind: 'Identifier', name: 'def' } as Identifier,
                value: { kind: 'NumericLiteral', value: 2 } as NumericLiteral
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration, variableDeclaration2, assignment]
        })
    })

    test("make AST string literal", () => {
        const ast = makeASTFromInput('"hello"')
        const stringLiteral = { kind: 'StringLiteral', value: 'hello' }

        expect(ast).toEqual({
            kind: 'Program',
            body: [stringLiteral]
        })
    })

    test("make AST string literal with escape character in variable", () => {
        const ast = makeASTFromInput('dec abc = "hello\nworld"')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'StringLiteral', value: 'hello\nworld' }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration]
        })
    })

    test("make AST print statement", () => {
        const ast = makeASTFromInput('ecrire 42')
        const printStatement = {
            kind: 'PrintStatement',
            value: { kind: 'NumericLiteral', value: 42 }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [printStatement]
        })
    })

    test("make AST if statement define block", () => {
        const ast = makeASTFromInput('si 42 alors\n  ecrire 42\nfin')
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST if else statement define blocks", () => {
        const code = [
            'si 42 alors',
            '  ecrire 42',
            'sinon',
            '  ecrire 2',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 2 } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST if else if statement", () => {
        const code = [
            'si 42 alors',
            '  ecrire 42',
            'sinonsi 2 alors',
            '  ecrire 2',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: {
                kind: 'IfStatement',
                condition: { kind: 'NumericLiteral', value: 2 },
                thenBranch: {
                    kind: 'BlockStatement',
                    body: [
                        { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 2 } }
                    ]
                }
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST if else if else statement", () => {
        const code = [
            'si 42 alors',
            '  ecrire 42',
            'sinonsi 2 alors',
            '  ecrire 2',
            'sinonsi 3 alors',
            '  ecrire 3',
            'sinon',
            '  ecrire 4',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: {
                kind: 'IfStatement',
                condition: { kind: 'NumericLiteral', value: 2 },
                thenBranch: {
                    kind: 'BlockStatement',
                    body: [
                        { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 2 } }
                    ]
                },
                elseBranch: {
                    kind: 'IfStatement',
                    condition: { kind: 'NumericLiteral', value: 3 },
                    thenBranch: {
                        kind: 'BlockStatement',
                        body: [
                            { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 3 } }
                        ]
                    },
                    elseBranch: {
                        kind: 'BlockStatement',
                        body: [
                            { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 4 } }
                        ]
                    }
                }
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST if else if else statement with nested if", () => {
        const code = [
            'si 42 alors',
            '  ecrire 42',
            'sinon',
            '  si 3 alors',
            '    ecrire 3',
            '  fin',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: {
                kind: 'BlockStatement',
                body: [
                    {
                        kind: 'IfStatement',
                        condition: { kind: 'NumericLiteral', value: 3 },
                        thenBranch: {
                            kind: 'BlockStatement',
                            body: [
                                { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 3 } }
                            ]
                        }
                    }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST IfStatement with multiple statements", () => {
        const code = [
            'si 42 alors',
            '  ecrire 42',
            '  ecrire 2',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: { kind: 'NumericLiteral', value: 42 },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } },
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 2 } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST IfStatement with et and ou", () => {
        const code = [
            'si a < b et a < c ou b < c alors',
            '  ecrire 42',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: {
                kind: 'BinaryExpression',
                left: {
                    kind: 'BinaryExpression',
                    left: {
                        kind: 'BinaryExpression',
                        left: { kind: 'Identifier', name: 'a' },
                        right: { kind: 'Identifier', name: 'b' },
                        operator: '<'
                    },
                    right: {
                        kind: 'BinaryExpression',
                        left: { kind: 'Identifier', name: 'a' },
                        right: { kind: 'Identifier', name: 'c' },
                        operator: '<'
                    },
                    operator: 'et',
                },
                right: {
                    kind: 'BinaryExpression',
                    left: { kind: 'Identifier', name: 'b' },
                    right: { kind: 'Identifier', name: 'c' },
                    operator: '<'
                },
                operator: 'ou'
            },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: undefined
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST IfStatement with et and ou with parenthesis", () => {
        const code = [
            'si a < b et (a < c ou b < c) alors',
            '  ecrire 42',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const ifStatement = {
            kind: 'IfStatement',
            condition: {
                kind: 'BinaryExpression',
                left: {
                    kind: 'BinaryExpression',
                    left: { kind: 'Identifier', name: 'a' },
                    right: { kind: 'Identifier', name: 'b' },
                    operator: '<'
                },
                right: {
                    kind: 'BinaryExpression',
                    left: {
                        kind: 'BinaryExpression',
                        left: { kind: 'Identifier', name: 'a' },
                        right: { kind: 'Identifier', name: 'c' },
                        operator: '<'
                    },
                    right: {
                        kind: 'BinaryExpression',
                        left: { kind: 'Identifier', name: 'b' },
                        right: { kind: 'Identifier', name: 'c' },
                        operator: '<'
                    },
                    operator: 'ou',
                },
                operator: 'et'
            },
            thenBranch: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            },
            elseBranch: undefined
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })
})
