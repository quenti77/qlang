import { TokenType, createToken, KEYWORDS, OPERATORS, type Token } from "./token"
import { IllegalCharError, StringUnterminatedError } from "./utils/errors"
import { Position } from "./utils/position"

export default class Lexer {

    private currentLine: string | undefined = undefined
    private position: Position = new Position(0, 0, 0, '')

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
            if (this.processOperator()) {
                this.pushToken(TokenType.BinaryOperator, this.src.shift()!)
            } else if (this.src[0] === '-') {
                this.pushToken(TokenType.UnaryOperator, this.src.shift()!)
            } else if (this.src[0] === '(') {
                this.pushToken(TokenType.OpenParenthesis, this.src.shift()!)
            } else if (this.src[0] === ')') {
                this.pushToken(TokenType.CloseParenthesis, this.src.shift()!)
            } else if (this.src[0] === '[') {
                this.pushToken(TokenType.OpenBrackets, this.src.shift()!)
            } else if (this.src[0] === ']') {
                this.pushToken(TokenType.CloseBrackets, this.src.shift()!)
            } else if (this.src[0] === ',') {
                this.pushToken(TokenType.Comma, this.src.shift()!)
            } else if (this.isStartLogicalOperator(this.src[0])) {
                let currentChar = this.src.shift()!
                if (this.src[0] === '=') {
                    currentChar += this.src.shift()!
                }
                this.pushToken(currentChar === '=' ? TokenType.Equals : TokenType.BinaryOperator, currentChar)
            } else if (this.src[0] === '.' || this.isNumber(this.src[0])) {
                this.processNumber()
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
                    if (token === 'rem') {
                        this.addCol(token)
                        while (this.src.length > 0) {
                            this.eat()
                        }
                        break
                    }
                    this.pushToken(TokenType.Identifier, token)
                }
            } else {
                this.position.advance(false, this.src.shift()!)
            }
        }
    }

    private processOperator(): boolean {
        if (this.isStartLogicalOperator(this.src[0])) {
            return false
        }
        if (!OPERATORS.includes(this.src[0])) {
            return false
        }

        if (this.src[0] === '-' && /[a-zA-Z0-9_]|\(|\)/.test(this.src[1] ?? '')) {
            return false
        }

        return true
    }

    private isStartLogicalOperator(char: string): boolean {
        return ['=', '!', '<', '>'].includes(char)
    }

    private processNumber(): void {
        let token = this.src.shift()!
        let hasDot = token === '.'

        while (this.hasMoreChars() && (this.isNumber(this.src[0]) || this.src[0] === '.')) {
            if (this.src[0] === '.') {
                if (hasDot) {
                    const posStart = this.position.copy()
                    this.addCol(token)
                    const posEnd = this.position.copy()

                    throw new IllegalCharError(posStart, posEnd, '.')
                }
                hasDot = true
            }
            token += this.src.shift()!
        }

        this.pushToken(TokenType.Number, token)
    }

    private processString(): void {
        this.eat()

        const posStart = this.position.copy()

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
                    default: {
                        this.addCol(currentChar)
                        const posEnd = this.position.copy()
                        throw new IllegalCharError(posStart, posEnd, nextChar ?? '')
                    }
                }
                continue
            }
            if (currentChar === undefined) {
                if (!this.hasMoreLines()) {
                    const posEnd = this.position.copy()
                    throw new StringUnterminatedError(posStart, posEnd)
                }
                this.nextLine()
                value += '\n'
                continue
            }
            value += currentChar
        }

        posStart.content = value

        this.tokens.push(createToken(TokenType.String, value, posStart))
        this.eat()
    }

    private eat(): string | undefined {
        const currentChar = this.src.shift()
        this.position.advance(false, currentChar ?? '')

        return currentChar
    }

    private addCol(char: string): void {
        this.position.advance(false, char)
    }

    private pushToken(
        type: TokenType,
        value: string,
        position: Position = this.position
    ) {
        const tokenPosition = position.copy()
        tokenPosition.content = value

        this.tokens.push(createToken(type, value, tokenPosition))
        this.position.advance(false, value)
    }

    private isIdentifier(token: string | undefined, withNumber: boolean): boolean {
        if (token === undefined) {
            return false
        }
        return withNumber
            ? /[a-zA-Z0-9_]/.test(token)
            : /[a-zA-Z_]/.test(token)
    }

    private isNumber(token: string | undefined): boolean {
        if (token === undefined) {
            return false
        }
        return token.charCodeAt(0) >= 48 && token.charCodeAt(0) <= 57
    }

    private hasMoreChars(): boolean {
        return this.src.length > 0
    }

    private hasMoreLines(): boolean {
        return this.lines.length > 0
    }

    private nextLine(): void {
        this.position.advance(true, '')
        this.currentLine = this.lines.shift()
        this.src = this.currentLine?.split('') ?? []
    }

    private reset(): void {
        this.position = new Position(0, 0, 0, '')
        this.lines = []
        this.tokens = []
        this.currentLine = undefined
    }
}
