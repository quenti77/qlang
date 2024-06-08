import type {
    ArrayExpression,
    AssignmentExpression,
    BinaryExpression,
    BlockStatement,
    BooleanLiteral,
    CallExpression,
    Expression,
    ForStatement,
    FunctionStatement,
    Identifier,
    IfStatement,
    MemberExpression,
    NullLiteral,
    NumericLiteral,
    PrintStatement,
    Program,
    ReturnStatement,
    Statement,
    StringLiteral,
    UnaryExpression,
    VariableDeclarationStatement,
    WhileStatement,
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
            case TokenType.While:
                return this.parseWhileStatement()
            case TokenType.For:
                return this.parseForStatement()
            case TokenType.Function:
                return this.parseFunctionDeclarationStatement()
            case TokenType.Break:
                this.eat()
                return { kind: 'BreakStatement' } as Statement
            case TokenType.Continue:
                this.eat()
                return { kind: 'ContinueStatement' } as Statement
            case TokenType.Return:
                return this.parseReturnStatement()
            default:
                return this.parseExpression()
        }
    }

    private parseVariableDeclarationStatement(): Statement {
        this.eatExactly(TokenType.Let, '')

        const identifier = this.eatExactly(TokenType.Identifier, '').value

        if (this.at().type === TokenType.Equals) {
            this.eatExactly(TokenType.Equals, '')
            return {
                kind: 'VariableDeclarationStatement',
                identifier,
                value: this.at().type === TokenType.Function ? this.parseFunctionDeclarationStatement() : this.parseExpression(),
            } as VariableDeclarationStatement
        }

        return {
            kind: 'VariableDeclarationStatement',
            identifier,
        } as VariableDeclarationStatement
    }

    private parsePrintStatement(): Statement {
        this.eatExactly(TokenType.Print, '')

        return {
            kind: 'PrintStatement',
            value: this.parseExpression(),
        } as PrintStatement
    }

    private parseIfStatement(endNeeded: boolean = true): Statement {
        if (endNeeded) {
            this.eatExactly(TokenType.If, '')
        }
        const condition = this.parseExpression()
        this.eatExactly(TokenType.Then, '')
        const thenBranch = this.parseBlockStatement([TokenType.Else, TokenType.ElseIf])

        let elseBranch: Statement | undefined = undefined
        if (this.at().type === TokenType.Else) {
            this.eatExactly(TokenType.Else, '')
            elseBranch = this.parseBlockStatement()
        } else if (this.at().type === TokenType.ElseIf) {
            this.eatExactly(TokenType.ElseIf, '')
            elseBranch = this.parseIfStatement(false)
        }

        if (endNeeded) {
            this.eatExactly(TokenType.End, '')
        }

        return {
            kind: 'IfStatement',
            condition,
            thenBranch,
            elseBranch,
        } as IfStatement
    }

    private parseWhileStatement(): Statement {
        this.eatExactly(TokenType.While, '')
        const condition = this.parseExpression()
        this.eatExactly(TokenType.Then, '')

        const body = this.parseBlockStatement()
        this.eatExactly(TokenType.End, '')

        return {
            kind: 'WhileStatement',
            condition,
            body,
        } as WhileStatement
    }

    private parseForStatement(): Statement {
        this.eatExactly(TokenType.For, '')
        const identifier = this.eatExactly(TokenType.Identifier, '')
        this.eatExactly(TokenType.From, '')
        const from = this.parseExpression()

        this.eatExactly(TokenType.Until, '')

        let until: Expression = this.parseExpression()
        if (until.kind === 'NumericLiteral' || until.kind === 'Identifier') {
            until = {
                kind: 'BinaryExpression',
                left: { kind: 'Identifier', name: identifier.value },
                right: until,
                operator: '<=',
            } as BinaryExpression
        }

        const token = this.at().type
        let step = { kind: 'NumericLiteral', value: 1 } as Expression
        if (token === TokenType.Step) {
            this.eatExactly(TokenType.Step, '')
            step = this.parseExpression()
        }

        step = {
            kind: 'AssignmentExpression',
            assignment: { kind: 'Identifier', name: identifier.value },
            value: {
                kind: 'BinaryExpression',
                left: { kind: 'Identifier', name: identifier.value },
                right: step,
                operator: '+',
            } as BinaryExpression,
        } as AssignmentExpression

        this.eatExactly(TokenType.Then, '')
        const body = this.parseBlockStatement()
        this.eatExactly(TokenType.End, '')

        return {
            kind: 'ForStatement',
            identifier: identifier.value,
            from,
            until,
            step,
            body,
        } as ForStatement
    }

    private parseFunctionDeclarationStatement(): Statement {
        this.eatExactly(TokenType.Function, '')
        const identifier = this.at().type === TokenType.OpenParenthesis
            ? null
            : this.eatExactly(TokenType.Identifier, '').value

        this.eatExactly(TokenType.OpenParenthesis, '')

        const params = []
        while (this.at().type !== TokenType.CloseParenthesis) {
            if (params.length >= 48) {
                // TODO: throw error
                throw new Error()
            }
            params.push(this.eatExactly(TokenType.Identifier, '').value)

            const token = this.attempt([TokenType.Comma, TokenType.CloseParenthesis], '')
            if (token.type === TokenType.Comma) {
                this.eat()
            }
        }

        this.eatExactly(TokenType.CloseParenthesis, '')
        const body = this.parseBlockStatement()
        this.eatExactly(TokenType.End, '')

        return {
            kind: 'FunctionStatement',
            identifier,
            parameters: params,
            body,
        } as FunctionStatement
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

    private parseReturnStatement(): Statement {
        this.eatExactly(TokenType.Return, '')

        return {
            kind: 'ReturnStatement',
            value: this.parseExpression(),
        } as ReturnStatement
    }

    private parseExpression(): Expression {
        return this.parseAssignmentExpression()
    }

    private parseAssignmentExpression(): Expression {
        const left = this.parseLogicalExpression()

        if (this.at().type !== TokenType.Equals) {
            return left
        }

        this.eatExactly(TokenType.Equals, '')
        return {
            kind: 'AssignmentExpression',
            assignment: left,
            value: this.parseAssignmentExpression(),
        } as AssignmentExpression
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

        return this.parseArrayAccessExpression()
    }

    private parseArrayAccessExpression(): Expression {
        let expression = this.parseArrayExpression()

        while (this.at().type === TokenType.OpenBrackets) {
            this.eatExactly(TokenType.OpenBrackets, '')
            if (this.at().type === TokenType.CloseBrackets) {
                this.eat()
                expression = {
                    kind: 'MemberExpression',
                    object: expression,
                    property: null,
                } as MemberExpression
                break
            }
            const index = this.parseExpression()
            this.eatExactly(TokenType.CloseBrackets, '')

            expression = {
                kind: 'MemberExpression',
                object: expression,
                property: index,
            } as MemberExpression
        }

        return expression
    }

    private parseArrayExpression(): Expression {
        if (this.at().type !== TokenType.OpenBrackets) {
            return this.parseCallExpression()
        }

        this.eatExactly(TokenType.OpenBrackets, '')

        const elements = []
        while (this.at().type !== TokenType.CloseBrackets) {
            elements.push(this.parseExpression())

            const token = this.attempt([TokenType.Comma, TokenType.CloseBrackets], '')
            if (token.type === TokenType.Comma) {
                this.eat()
            }
        }

        this.eatExactly(TokenType.CloseBrackets, '')

        return {
            kind: 'ArrayExpression',
            elements,
        } as ArrayExpression
    }

    private parseCallExpression(): Expression {
        let expression = this.parsePrimaryExpression()

        while (this.at().type === TokenType.OpenParenthesis) {
            this.eatExactly(TokenType.OpenParenthesis, '')

            const args = []
            while (this.at().type !== TokenType.CloseParenthesis) {
                args.push(this.at().type === TokenType.Function ? this.parseFunctionDeclarationStatement() : this.parseExpression())

                const token = this.attempt([TokenType.Comma, TokenType.CloseParenthesis], '')
                if (token.type === TokenType.Comma) {
                    this.eat()
                }
            }

            this.eatExactly(TokenType.CloseParenthesis, '')

            expression = {
                kind: 'CallExpression',
                callee: expression,
                arguments: args,
            } as CallExpression
        }

        return expression
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
                this.eatExactly(TokenType.CloseParenthesis, '')

                return expression
            }
            default:
                // TODO: throw error
                throw new Error()
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
            // TODO: throw error
            throw new Error(err)
        }

        return token
    }

    private attempt(types: TokenType[], err: string): Token {
        const token = this.at()

        if (types.includes(token.type)) {
            return token
        }

        throw new Error(err)
    }

}
