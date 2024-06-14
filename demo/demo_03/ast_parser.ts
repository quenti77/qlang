import type {
    Statement,
    Program,
    Expression,
    BinaryExpression,
    UnaryExpression,
    NumericLiteral,
} from './ast'
import { BINARY_OPERATOR, TOKEN, type IToken } from './token'

export default class AstParser {

    private tokens: IToken[] = []

    public parse(tokens: IToken[]): Program {
        this.tokens = tokens

        const program: Program = {
            kind: 'Program',
            body: []
        }

        while (this.isEOF() === false) {
            program.body.push(this.parseStatement())
        }

        return program
    }

    // Parser methods order by precedence
    private parseStatement(): Statement {
        return this.parseExpression()
    }

    private parseExpression(): Expression {
        return this.parseAdditiveExpression()
    }

    private parseAdditiveExpression(): Expression {
        let expression = this.parseMultiplicativeExpression()

        while ([BINARY_OPERATOR.PLUS, BINARY_OPERATOR.MINUS].includes(this.at().value as BINARY_OPERATOR)) {
            const operator = this.eat().value
            const right = this.parseMultiplicativeExpression()

            expression = {
                kind: 'BinaryExpression',
                operator,
                left: expression,
                right
            } as BinaryExpression
        }

        return expression
    }

    private parseMultiplicativeExpression(): Expression {
        let expression = this.parseUnaryExpression()

        while ([BINARY_OPERATOR.MULTIPLY, BINARY_OPERATOR.DIVIDE].includes(this.at().value as BINARY_OPERATOR)) {
            const operator = this.eat().value
            const right = this.parseUnaryExpression()

            expression = {
                kind: 'BinaryExpression',
                operator,
                left: expression,
                right
            } as BinaryExpression
        }

        return expression
    }

    private parseUnaryExpression(): Expression {
        if (this.at().value === BINARY_OPERATOR.MINUS) {
            this.eat()
            const value = this.parsePrimaryExpression()

            return {
                kind: 'UnaryExpression',
                operator: BINARY_OPERATOR.MINUS,
                value
            } as UnaryExpression
        }

        return this.parsePrimaryExpression()
    }

    private parsePrimaryExpression(): Expression {
        const tokenType = this.at().type

        switch (tokenType) {
            case TOKEN.NUMBER:
                return { kind: 'NumericLiteral', value: parseFloat(this.eat().value) } as NumericLiteral
            case TOKEN.OPEN_PARENTHESIS:
                this.eat()
                const expression = this.parseExpression()
                this.eatExactly(TOKEN.CLOSE_PARENTHESIS)

                return expression
            default:
                throw new Error(`Unexpected token ${tokenType}`)
        }
    }

    private isEOF(): boolean {
        return this.at().type === TOKEN.EOF
    }

    private eat(): IToken {
        return this.tokens.shift()!
    }

    private eatExactly(type: TOKEN): void {
        const token = this.eat()
        if (token.type !== type) {
            throw new Error(`Unexpected token ${token.type} at line ${token.row}, col ${token.col}`)
        }
    }

    private at(): IToken {
        return this.tokens[0] ?? { type: TOKEN.EOF, value: '', row: 0, col: 0 }
    }

}
