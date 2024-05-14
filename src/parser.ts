import type {
    AssignmentExpression,
    BinaryExpression,
    BooleanLiteral,
    Expression,
    Identifier,
    NullLiteral,
    NumericLiteral,
    Program,
    Statement,
    StringLiteral,
    VariableDeclarationStatement,
} from "./ast"
import { TokenType, type Token } from "./token"

export default class Parser {
    private tokens: Token[]

    constructor() {
        this.tokens = []
    }

    public setTokens(tokens: Token[]): void {
        this.tokens = tokens
    }

    public makeAST(): Program {
        const program: Program = {
            kind: 'Program',
            body: [],
        }

        while (this.isEOF() === false) {
            program.body.push(this.parseStatement())
        }

        return program
    }

    // Parser methods order by precedence
    private parseStatement(): Statement {
        switch (this.at().type) {
            case TokenType.Let:
                return this.parseVariableDeclarationStatement()
            default:
                return this.parseExpression()
        }
    }

    private parseVariableDeclarationStatement(): Statement {
        this.eatExactly(TokenType.Let, 'Expected "let" keyword')

        const identifier = this.eatExactly(TokenType.Identifier, 'Expected identifier').value

        if (this.at().type === TokenType.Equals) {
            this.eatExactly(TokenType.Equals, 'Expected "="')
            return {
                kind: 'VariableDeclarationStatement',
                identifier,
                value: this.parseExpression(),
            } as VariableDeclarationStatement
        }

        return {
            kind: 'VariableDeclarationStatement',
            identifier,
        } as VariableDeclarationStatement
    }

    private parseExpression(): Expression {
        return this.parseAssignmentExpression()
    }

    private parseAssignmentExpression(): Expression {
        const left = this.parseAdditiveExpression()

        if (this.at().type === TokenType.Equals) {
            this.eatExactly(TokenType.Equals, 'Expected "="')

            return {
                kind: 'AssignmentExpression',
                assignment: left,
                value: this.parseAssignmentExpression(),
            } as AssignmentExpression
        }
        return left
    }

    private parseAdditiveExpression(): Expression {
        let left = this.parseMultiplicitaveExpression()

        while (['+', '-'].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parseMultiplicitaveExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
        }

        return left
    }

    private parseMultiplicitaveExpression(): Expression {
        let left = this.parsePrimaryExpression()

        while (['*', '/', '%'].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parsePrimaryExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
        }

        return left
    }

    private parsePrimaryExpression(): Expression {
        const tokenType = this.at().type

        switch (tokenType) {
            case TokenType.Identifier:
                return { kind: "Identifier", name: this.eat().value } as Identifier
            case TokenType.Null:
                this.eat()
                return { kind: "NullLiteral", value: 'null' } as NullLiteral
            case TokenType.Boolean:
                return { kind: "BooleanLiteral", value: this.eat().value === 'vrai' } as BooleanLiteral
            case TokenType.Number:
                return { kind: "NumericLiteral", value: parseFloat(this.eat().value) } as NumericLiteral
            case TokenType.String:
                return { kind: "StringLiteral", value: this.eat().value } as StringLiteral
            case TokenType.OpenParenthesis:
                this.eat()
                const expression = this.parseExpression()
                this.eatExactly(TokenType.CloseParenthesis, 'Expected closing parenthesis')

                return expression
            default:
                throw new Error(`Unexpected token type: ${tokenType}`)
        }
    }

    // Utilities methods
    private isEOF(): boolean {
        return this.at().type === TokenType.EOF
    }

    private at(): Token {
        return this.tokens[0] ?? { type: TokenType.EOF, value: '', line: 0, column: 0 }
    }

    private eat(): Token {
        return this.tokens.shift()!
    }

    private eatExactly(type: TokenType, err: string): Token {
        const token = this.eat()

        if (!token || token.type !== type) {
            throw new Error(err)
        }

        return token
    }
}
