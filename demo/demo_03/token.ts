
export enum TOKEN {
    NUMBER = 'number',
    UNARY_OPERATOR = 'unary_operator',
    BINARY_OPERATOR = 'binary_operator',
    OPEN_PARENTHESIS = 'open_parenthesis',
    CLOSE_PARENTHESIS = 'close_parenthesis',
    EOF = 'eof',
}

export enum BINARY_OPERATOR {
    PLUS = '+',
    MINUS = '-',
    MULTIPLY = '*',
    DIVIDE = '/',
    MODULUS = '%',
}
export const OPERATORS = Object.values(BINARY_OPERATOR)

export interface IToken {
    type: TOKEN
    value: string
    row: number
    col: number
}

export function createToken(type: TOKEN, value: string, row: number, col: number): IToken {
    return { type, value, row, col }
}
