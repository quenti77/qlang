import { TokenType, createToken, KEYWORDS, OPERATORS, type Token } from "./token"

export default class Lexer {

    private currentLine: string | undefined = undefined
    private row = 1
    private col = 1

    private lines: string[] = []
    private tokens: Token[] = []
    private src: string[] = []

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

        this.src = this.currentLine.split('')

        while (this.src.length > 0) {
            if (OPERATORS.includes(this.src[0]) && !this.isStartLogicalOperator(this.src[0])) {
                this.pushToken(TokenType.BinaryOperator, this.src.shift()!)
            } else if (this.src[0] === '(') {
                this.pushToken(TokenType.OpenParenthesis, this.src.shift()!)
            } else if (this.src[0] === ')') {
                this.pushToken(TokenType.CloseParenthesis, this.src.shift()!)
            } else if (this.isStartLogicalOperator(this.src[0])) {
                let currentChar = this.src.shift()!
                if (this.src[0] === '=') {
                    currentChar += this.src.shift()!
                }
                this.pushToken(currentChar === '=' ? TokenType.Equals : TokenType.BinaryOperator, currentChar)
            } else if (/[0-9]/.test(this.src[0])) {
                let number = this.src.shift()!
                while (/[0-9]/.test(this.src[0])) {
                    number += this.src.shift()!
                }
                this.pushToken(TokenType.Number, number)
            } else if (this.src[0] === '"') {
                this.processString()
            } else if (this.isIdentifier(this.src[0], false)) {
                let token = this.src.shift()!
                while (this.isIdentifier(this.src[0], true)) {
                    token += this.src.shift()!
                }

                const reserved = KEYWORDS[token]
                if (typeof reserved === 'string') {
                    this.pushToken(reserved, token)
                } else {
                    this.pushToken(TokenType.Identifier, token)
                }
            } else {
                this.src.shift()
                this.col++
            }
        }
    }

    private isStartLogicalOperator(char: string): boolean {
        return ['=', '!', '<', '>'].includes(char)
    }

    private processString(): void {
        this.eat()

        const startLine = this.row
        const startColumn = this.col

        let value = ''
        while (this.src[0] !== '"') {
            const currentChar = this.eat()
            if (currentChar === '\\') {
                const nextChar = this.src.shift()
                switch (nextChar) {
                    case '\\':
                        value += '\\'
                        break
                    case '"':
                        value += '"'
                        break
                    case 'n':
                        this.nextLine()
                        break
                    default:
                        throw new Error(`Unknown escape sequence: \\${nextChar}`)
                }
                continue
            }
            if (currentChar === undefined) {
                if (!this.hasMoreLines()) {
                    throw new Error('Unterminated string')
                }
                this.nextLine()
                value += '\n'
                continue
            }
            value += currentChar
        }

        this.tokens.push(createToken(TokenType.String, value, startLine, startColumn))
        this.eat()
    }

    private eat(): string | undefined {
        const currentChar = this.src.shift()
        this.col++

        return currentChar
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
        this.src = this.currentLine?.split('') ?? []
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
