import { TOKEN, BINARY_OPERATOR, type IToken, createToken, OPERATORS } from "./token"

const NUMBERS = '0123456789'
const LETTERS = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'
const START_IDENTIFIERS = LETTERS + '_'
const IDENTIFIERS = START_IDENTIFIERS + NUMBERS

export default class Lexer {

    private row: number = 0
    private col: number = 0

    private code: string[] = []
    private tokens: IToken[] = []

    public tokenize(input: string): IToken[] {
        this.reset()
        this.code = input.split('')

        while (this.hasMoreTokens()) {
            this.processNextToken()
        }

        return this.tokens
    }

    private processNextToken(): void {
        if (this.code[0] === '(') {
            this.pushToken(TOKEN.OPEN_PARENTHESIS, this.eat())
        } else if (this.code[0] === ')') {
            this.pushToken(TOKEN.CLOSE_PARENTHESIS, this.eat())
        } else if (this.code[0] === '=') {
            this.pushToken(TOKEN.EQUALS, this.eat())
        } else if (START_IDENTIFIERS.includes(this.code[0])) {
            this.processIdentifier()
        } else if (OPERATORS.includes(this.code[0] as BINARY_OPERATOR)) {
            this.pushToken(TOKEN.BINARY_OPERATOR, this.eat())
        } else if (NUMBERS.includes(this.code[0])) {
            this.processNumber()
        } else {
            this.eat()
        }
    }

    private processIdentifier(): void {
        let identifier = this.eat()

        while (IDENTIFIERS.includes(this.code[0])) {
            identifier += this.eat()
        }

        this.pushToken(TOKEN.IDENTIFIER, identifier)
    }

    private processNumber(): void {
        let number = this.eat()
        let countDot = 0

        while (NUMBERS.includes(this.code[0]) || this.code[0] === '.') {
            const char = this.eat()
            if (char === '.') {
                if (countDot > 0) {
                    throw new Error('Invalid number')
                }
                countDot++
            }
            number += char
        }

        this.pushToken(TOKEN.NUMBER, number)
    }

    private pushToken(type: TOKEN, value: string): void {
        this.tokens.push(createToken(type, value, this.row, this.col))
    }

    private eat(): string {
        const char = this.code.shift()!

        this.col++
        if (char === '\n') {
            this.row++
            this.col = 0
        }

        return char
    }

    private hasMoreTokens(): boolean {
        return this.code.length > 0
    }

    private reset(): void {
        this.row = 0
        this.col = 0
        this.tokens = []
    }

}
