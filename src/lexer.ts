import { TokenType, createToken, KEYWORDS, OPERATORS, type Token } from "./token"

export default class Lexer {

    private currentLine: string | undefined = undefined
    private row = 1
    private col = 1

    private lines: string[] = []
    private tokens: Token[] = []

    public get Tokens(): Token[] {
        return this.tokens
    }

    public tokenize(input: string): void {
        this.reset()
        this.lines = input.split('\n')

        while (this.hasMoreLines()) {
            this.nextLine()
            this.tokenizeLine()
        }
        this.pushToken(TokenType.EOF, '')
    }

    private tokenizeLine() {
        if (this.currentLine === undefined) {
            return
        }

        let src = this.currentLine.split('')

        while (src.length > 0) {
            if (OPERATORS.includes(src[0])) {
                this.pushToken(TokenType.BinaryOperator, src.shift()!)
            } else if (src[0] === '(') {
                this.pushToken(TokenType.OpenParenthesis, src.shift()!)
            } else if (src[0] === ')') {
                this.pushToken(TokenType.CloseParenthesis, src.shift()!)
            } else if (src[0] === '=') {
                this.pushToken(TokenType.Equals, src.shift()!)
            } else if (/[0-9]/.test(src[0])) {
                let number = src.shift()!
                while (/[0-9]/.test(src[0])) {
                    number += src.shift()!
                }
                this.pushToken(TokenType.Number, number)
            } else if (this.isIdentifier(src[0], false)) {
                let token = src.shift()!
                while (this.isIdentifier(src[0], true)) {
                    token += src.shift()!
                }

                const reserved = KEYWORDS[token]
                if (reserved !== undefined) {
                    this.pushToken(reserved, token)
                } else {
                    this.pushToken(TokenType.Identifier, token)
                }
            } else {
                src.shift()
                this.col++
            }
        }
    }

    private pushToken(type: TokenType, value: string, line: number = this.row, column: number = this.col) {
        this.tokens.push(createToken(type, value, line, column))
        this.col += value.length
    }

    private isIdentifier(token: string | undefined, withNumber: boolean): boolean {
        if (token === undefined) {
            return false
        }
        return withNumber
            ? /[a-zA-Z0-9_]/.test(token)
            : /[a-zA-Z_]/.test(token)
    }

    private hasMoreLines(): boolean {
        return this.lines.length > 0
    }

    private nextLine(): void {
        if (this.currentLine !== undefined) {
            this.row++
        }
        this.currentLine = this.lines.shift()
        this.col = 1
    }

    private reset(): void {
        this.row = 1
        this.col = 1
        this.lines = []
        this.tokens = []
        this.currentLine = undefined
    }
}
