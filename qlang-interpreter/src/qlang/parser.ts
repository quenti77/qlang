import type {
    AssignmentExpression,
    BinaryExpression,
    BlockStatement,
    BooleanLiteral,
    Expression,
    Identifier,
    IfStatement,
    NullLiteral,
    NumericLiteral,
    PrintStatement,
    Program,
    Statement,
    StringLiteral,
    UnaryExpression,
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
            case TokenType.Print:
                return this.parsePrintStatement()
            case TokenType.If:
                return this.parseIfStatement()
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

    private parsePrintStatement(): Statement {
        this.eatExactly(TokenType.Print, 'Expected "ecrire" keyword')

        return {
            kind: 'PrintStatement',
            value: this.parseExpression(),
        } as PrintStatement
    }

    private parseIfStatement(endNeeded: boolean = true): Statement {
        if (endNeeded) {
            this.eatExactly(TokenType.If, 'Expected "si" keyword')
        }
        const condition = this.parseExpression()
        this.eatExactly(TokenType.Then, 'Expected "alors" keyword')
        const thenBranch = this.parseBlockStatement([TokenType.Else, TokenType.ElseIf])

        let elseBranch: Statement | undefined = undefined
        if (this.at().type === TokenType.Else) {
            this.eatExactly(TokenType.Else, 'Expected "sinon" keyword')
            elseBranch = this.parseBlockStatement()
        } else if (this.at().type === TokenType.ElseIf) {
            this.eatExactly(TokenType.ElseIf, 'Expected "sinonsi" keyword')
            elseBranch = this.parseIfStatement(false)
        }

        if (endNeeded) {
            this.eatExactly(TokenType.End, 'Expected "fin" keyword')
        }

        return {
            kind: 'IfStatement',
            condition,
            thenBranch,
            elseBranch,
        } as IfStatement
    }

    private parseBlockStatement(withCondition: TokenType[] = []): BlockStatement {
        const block: BlockStatement = {
            kind: 'BlockStatement',
            body: [],
        }

        while (
            this.isEOF() === false &&
            this.at().type !== TokenType.End &&
            withCondition.includes(this.at().type) === false
        ) {
            block.body.push(this.parseStatement())
        }

        return block
    }

    private parseExpression(): Expression {
        return this.parseAssignmentExpression()
    }

    private parseAssignmentExpression(): Expression {
        const left = this.parseLogicalExpression()

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

    private parseLogicalExpression(): Expression {
        let left = this.parseEqualityExpression()

        while (['et', 'ou'].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parseEqualityExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
        }

        return left
    }

    private parseEqualityExpression(): Expression {
        let left = this.parseRelationalExpression()

        while (['==', '!='].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parseRelationalExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
        }

        return left
    }

    private parseRelationalExpression(): Expression {
        let left = this.parseAdditiveExpression()

        while (['>', '<', '>=', '<='].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parseAdditiveExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
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
        let left = this.parseUnaryExpression()

        while (['*', '/', '%'].includes(this.at().value)) {
            const operator = this.eat().value
            const right = this.parseUnaryExpression()

            left = {
                kind: 'BinaryExpression',
                left,
                right,
                operator,
            } as BinaryExpression
        }

        return left
    }

    private parseUnaryExpression(): Expression {
        if (this.at().type === TokenType.UnaryOperator) {
            const operator = this.eat().value
            const value = this.parseUnaryExpression()

            return {
                kind: 'UnaryExpression',
                operator,
                value,
            } as UnaryExpression
        }

        return this.parsePrimaryExpression()
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
            case TokenType.OpenParenthesis: {
                this.eat()
                const expression = this.parseExpression()
                this.eatExactly(TokenType.CloseParenthesis, 'Expected closing parenthesis')

                return expression
            }
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
