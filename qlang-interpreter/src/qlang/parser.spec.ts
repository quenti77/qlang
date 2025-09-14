import { expect, test, describe, beforeEach } from "bun:test"

import Parser from "./parser"
import Lexer from "./lexer"
import type {
    ArrayExpression,
    AssignmentExpression,
    BinaryExpression,
    BooleanLiteral,
    CallExpression,
    ForStatement,
    FunctionStatement,
    Identifier,
    IfStatement,
    MemberExpression,
    NullLiteral,
    NumericLiteral,
    PrintStatement,
    Program, ReadExpression,
    ReturnStatement,
    StringLiteral,
    VariableDeclarationStatement,
    WhileStatement,
} from './ast'
import { OPERATORS } from "./token"

describe("Parser", () => {
    let lexer: Lexer
    let parser: Parser

    const makeASTFromInput = (input: string): Program => {
        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens, input)
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

    test("make AST simple float numeric literal", () => {
        const ast = makeASTFromInput('42.42')
        const numericLiteral: NumericLiteral = { kind: 'NumericLiteral', value: 42.42 }

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
        } as BinaryExpression

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
        } as BinaryExpression

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
            body: [booleanLiteral as BooleanLiteral]
        })
    })

    test("make AST variable declaration", () => {
        const ast = makeASTFromInput('dec abc = 42')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'NumericLiteral', value: 42 } as NumericLiteral
        } as VariableDeclarationStatement

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
        } as VariableDeclarationStatement

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
        } as VariableDeclarationStatement

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
        } as VariableDeclarationStatement
        const assignment = {
            kind: 'AssignmentExpression',
            assignment: { kind: 'Identifier', name: 'abc' } as Identifier,
            value: { kind: 'NumericLiteral', value: 2 } as NumericLiteral
        } as AssignmentExpression

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
        } as VariableDeclarationStatement
        const variableDeclaration2 = {
            kind: 'VariableDeclarationStatement',
            identifier: 'def',
            value: { kind: 'Identifier', name: 'abc' } as Identifier
        } as VariableDeclarationStatement
        const assignment = {
            kind: 'AssignmentExpression',
            assignment: { kind: 'Identifier', name: 'abc' } as Identifier,
            value: {
                kind: 'AssignmentExpression',
                assignment: { kind: 'Identifier', name: 'def' } as Identifier,
                value: { kind: 'NumericLiteral', value: 2 } as NumericLiteral
            }
        } as AssignmentExpression

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
            body: [stringLiteral as StringLiteral]
        })
    })

    test("make AST string literal with escape character in variable", () => {
        const ast = makeASTFromInput('dec abc = "hello\nworld"')
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: { kind: 'StringLiteral', value: 'hello\nworld' }
        } as VariableDeclarationStatement

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
        } as PrintStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [printStatement]
        })
    })

    test("make AST read expression", () => {
        const ast = makeASTFromInput('lire "Nom :"')
        const readExpression = {
            kind: 'ReadExpression',
            value: { kind: 'StringLiteral', value: "Nom :" }
        } as ReadExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [readExpression]
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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

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
        } as IfStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [ifStatement]
        })
    })

    test("make AST while statement", () => {
        const code = [
            'tantque vrai alors',
            '  ecrire 42',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const whileStatement = {
            kind: 'WhileStatement',
            condition: { kind: 'BooleanLiteral', value: true },
            body: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'NumericLiteral', value: 42 } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [whileStatement as WhileStatement]
        })
    })

    test("make AST for statement", () => {
        const code = [
            'pour abc de 1 jusque 10 evol 2 alors',
            '  ecrire abc',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const forStatement = {
            kind: 'ForStatement',
            identifier: 'abc',
            from: { kind: 'NumericLiteral', value: 1 },
            until: {
                kind: 'BinaryExpression',
                left: { kind: 'Identifier', name: 'abc' },
                right: { kind: 'NumericLiteral', value: 10 },
                operator: '<='
            },
            step: {
                kind: 'AssignmentExpression',
                assignment: { kind: 'Identifier', name: 'abc' },
                value: {
                    kind: 'BinaryExpression',
                    left: { kind: 'Identifier', name: 'abc' },
                    right: { kind: 'NumericLiteral', value: 2 },
                    operator: '+'
                }
            },
            body: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'abc' } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [forStatement as ForStatement]
        })
    })

    test("make AST for statement without evol", () => {
        const code = [
            'pour abc de 1 jusque 10 alors',
            '  ecrire abc',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const forStatement = {
            kind: 'ForStatement',
            identifier: 'abc',
            from: { kind: 'NumericLiteral', value: 1 },
            until: {
                kind: 'BinaryExpression',
                left: { kind: 'Identifier', name: 'abc' },
                right: { kind: 'NumericLiteral', value: 10 },
                operator: '<='
            },
            step: {
                kind: 'AssignmentExpression',
                assignment: { kind: 'Identifier', name: 'abc' },
                value: {
                    kind: 'BinaryExpression',
                    left: { kind: 'Identifier', name: 'abc' },
                    right: { kind: 'NumericLiteral', value: 1 },
                    operator: '+'
                }
            },
            body: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'abc' } }
                ]
            }
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [forStatement as ForStatement]
        })
    })

    test("make AST for simple array expression", () => {
        const ast = makeASTFromInput('[1, 2, 3]')
        const arrayExpression = {
            kind: 'ArrayExpression',
            elements: [
                { kind: 'NumericLiteral', value: 1 },
                { kind: 'NumericLiteral', value: 2 },
                { kind: 'NumericLiteral', value: 3 }
            ]
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [arrayExpression as ArrayExpression]
        })
    })

    test("make AST for nested array expression", () => {
        const ast = makeASTFromInput('[1, [2, 3,], 4,]')
        const arrayExpression = {
            kind: 'ArrayExpression',
            elements: [
                { kind: 'NumericLiteral', value: 1 },
                {
                    kind: 'ArrayExpression',
                    elements: [
                        { kind: 'NumericLiteral', value: 2 },
                        { kind: 'NumericLiteral', value: 3 }
                    ]
                },
                { kind: 'NumericLiteral', value: 4 }
            ]
        }

        expect(ast).toEqual({
            kind: 'Program',
            body: [arrayExpression as ArrayExpression]
        })
    })

    const BAD_ARRAY_EXPRESSIONS = [
        '[1, 2, 3',
        '[1, 2 3]',
        '[1, 2, 3[]',
    ]
    test.each(BAD_ARRAY_EXPRESSIONS)("make AST with bad syntax into array expression", (code: string) => {
        expect(() => makeASTFromInput(code)).toThrow()
    })

    test("make AST access array expression from variable", () => {
        const code = [
            'dec abc = [1, 2, 3]',
            'ecrire abc[1]'
        ]
        const ast = makeASTFromInput(code.join('\n'))
        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: {
                kind: 'ArrayExpression',
                elements: [
                    { kind: 'NumericLiteral', value: 1 },
                    { kind: 'NumericLiteral', value: 2 },
                    { kind: 'NumericLiteral', value: 3 }
                ]
            }
        } as VariableDeclarationStatement

        const printStatement = {
            kind: 'PrintStatement',
            value: {
                kind: 'MemberExpression',
                object: { kind: 'Identifier', name: 'abc' },
                property: { kind: 'NumericLiteral', value: 1 }
            }
        } as PrintStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration, printStatement]
        })
    })

    test("make AST access array expression from array expression", () => {
        const ast = makeASTFromInput('[1, 2, 3][1]')
        const arrayExpression = {
            kind: 'ArrayExpression',
            elements: [
                { kind: 'NumericLiteral', value: 1 } as NumericLiteral,
                { kind: 'NumericLiteral', value: 2 },
                { kind: 'NumericLiteral', value: 3 }
            ]
        } as ArrayExpression
        const memberExpression = {
            kind: 'MemberExpression',
            object: arrayExpression,
            property: { kind: 'NumericLiteral', value: 1 }
        } as MemberExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [memberExpression]
        })
    })

    test("make AST access array expression from array expression with variable", () => {
        const code = [
            'dec tab = [1, [2, 3], 4]',
            'dec index = 1',
            'tab[index][index]'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const tabDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'tab',
            value: {
                kind: 'ArrayExpression',
                elements: [
                    { kind: 'NumericLiteral', value: 1 },
                    {
                        kind: 'ArrayExpression',
                        elements: [
                            { kind: 'NumericLiteral', value: 2 },
                            { kind: 'NumericLiteral', value: 3 }
                        ]
                    },
                    { kind: 'NumericLiteral', value: 4 }
                ]
            }
        } as VariableDeclarationStatement

        const indexDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'index',
            value: { kind: 'NumericLiteral', value: 1 }
        } as VariableDeclarationStatement

        const memberExpression = {
            kind: 'MemberExpression',
            object: {
                kind: 'MemberExpression',
                object: { kind: 'Identifier', name: 'tab' },
                property: { kind: 'Identifier', name: 'index' }
            },
            property: { kind: 'Identifier', name: 'index' }
        } as MemberExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [tabDeclaration, indexDeclaration, memberExpression]
        })
    })

    test("make AST access to push new element into array", () => {
        const code = [
            'dec tab = []',
            'tab[] = 1'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const tabDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'tab',
            value: {
                kind: 'ArrayExpression',
                elements: []
            }
        } as VariableDeclarationStatement

        const assignment = {
            kind: 'AssignmentExpression',
            assignment: {
                kind: 'MemberExpression',
                object: { kind: 'Identifier', name: 'tab' },
                property: null
            },
            value: { kind: 'NumericLiteral', value: 1 }
        } as AssignmentExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [tabDeclaration, assignment]
        })
    })

    test("make AST access to push new element into sub array", () => {
        const code = [
            'dec tab = [[]]',
            'tab[0][] = 1'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const tabDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'tab',
            value: {
                kind: 'ArrayExpression',
                elements: [
                    { kind: 'ArrayExpression', elements: [] }
                ]
            }
        } as VariableDeclarationStatement

        const assignment = {
            kind: 'AssignmentExpression',
            assignment: {
                kind: 'MemberExpression',
                object: {
                    kind: 'MemberExpression',
                    object: { kind: 'Identifier', name: 'tab' },
                    property: { kind: 'NumericLiteral', value: 0 }
                },
                property: null
            },
            value: { kind: 'NumericLiteral', value: 1 }
        } as AssignmentExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [tabDeclaration, assignment]
        })
    })

    test("make AST call function with empty parameters", () => {
        const ast = makeASTFromInput('abc()')
        const callExpression = {
            kind: 'CallExpression',
            callee: { kind: 'Identifier', name: 'abc' },
            arguments: []
        } as CallExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [callExpression]
        })
    })

    test("make AST call function with one parameter value", () => {
        const ast = makeASTFromInput('abc(42)')
        const callExpression = {
            kind: 'CallExpression',
            callee: { kind: 'Identifier', name: 'abc' },
            arguments: [{ kind: 'NumericLiteral', value: 42 } as NumericLiteral]
        } as CallExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [callExpression]
        })
    })

    test("make AST call function with multiple parameters", () => {
        const ast = makeASTFromInput('abc(42, "hello", a)')
        const callExpression = {
            kind: 'CallExpression',
            callee: { kind: 'Identifier', name: 'abc' },
            arguments: [
                { kind: 'NumericLiteral', value: 42 },
                { kind: 'StringLiteral', value: 'hello' },
                { kind: 'Identifier', name: 'a' } as Identifier
            ]
        } as CallExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [callExpression]
        })
    })

    test("make AST function declaration without parameters", () => {
        const code = [
            'fonction abc()',
            '  ecrire 42',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const functionDeclaration = {
            kind: 'FunctionStatement',
            identifier: 'abc',
            parameters: [],
            body: {
                kind: 'BlockStatement',
                body: [
                    {
                        kind: 'PrintStatement',
                        value: { kind: 'NumericLiteral', value: 42 },
                    } as PrintStatement
                ]
            }
        } as FunctionStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [functionDeclaration]
        })
    })

    test("make AST function declaration with parameters and more lines", () => {
        const code = [
            'fonction addition(a, b)',
            '    ecrire a',
            '    ecrire b',
            '    retour a + b',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const functionDeclaration = {
            kind: 'FunctionStatement',
            identifier: 'addition',
            parameters: ['a', 'b'],
            body: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'a' } },
                    { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'b' } },
                    {
                        kind: 'ReturnStatement',
                        value: {
                            kind: 'BinaryExpression',
                            left: { kind: 'Identifier', name: 'a' },
                            right: { kind: 'Identifier', name: 'b' },
                            operator: '+'
                        }
                    } as ReturnStatement
                ]
            }
        } as FunctionStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [functionDeclaration]
        })
    })

    test("make AST function without identifier", () => {
        const code = [
            'fonction (a)',
            '    ecrire a',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const functionDeclaration = {
            kind: 'FunctionStatement',
            identifier: null,
            parameters: ['a'],
            body: {
                kind: 'BlockStatement',
                body: [
                    { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'a' } } as PrintStatement
                ]
            }
        } as FunctionStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [functionDeclaration]
        })
    })

    test("make AST variable value is a function without identifier", () => {
        const code = [
            'dec abc = fonction (a)',
            '    ecrire a',
            'fin'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const variableDeclaration = {
            kind: 'VariableDeclarationStatement',
            identifier: 'abc',
            value: {
                kind: 'FunctionStatement',
                identifier: null,
                parameters: ['a'],
                body: {
                    kind: 'BlockStatement',
                    body: [
                        { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'a' } } as PrintStatement
                    ]
                }
            }
        } as VariableDeclarationStatement

        expect(ast).toEqual({
            kind: 'Program',
            body: [variableDeclaration]
        })
    })

    test("make AST function call with anonymous function", () => {
        const code = [
            'abc(fonction (a)',
            '    ecrire a',
            'fin)'
        ]
        const ast = makeASTFromInput(code.join('\n'))

        const callExpression = {
            kind: 'CallExpression',
            callee: {
                kind: 'Identifier',
                name: 'abc'
            },
            arguments: [
                {
                    kind: 'FunctionStatement',
                    identifier: null,
                    parameters: ['a'],
                    body: {
                        kind: 'BlockStatement',
                        body: [
                            { kind: 'PrintStatement', value: { kind: 'Identifier', name: 'a' } } as PrintStatement
                        ]
                    }
                } as FunctionStatement
            ]
        } as CallExpression

        expect(ast).toEqual({
            kind: 'Program',
            body: [callExpression]
        })
    })
})
