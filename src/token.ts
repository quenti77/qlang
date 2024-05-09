export enum TokenType {
    Number = 'number',
    Identifier = 'identifier',
    BinaryOperator = 'binary_operator',
    Equals = 'equals',
    OpenParenthesis = 'open_parenthesis',
    CloseParenthesis = 'close_parenthesis',
    If = 'if',
    Then = 'then',
    End = 'end',
    EOF = 'eof',
}

export interface Token {
    type: TokenType
    value: string
    line: number
    column: number
}

export function createToken(type: TokenType, value: string, line: number, column: number): Token {
    return { type, value, line, column }
}

export const KEYWORDS: Record<string, TokenType> = {
    'si': TokenType.If,
    'alors': TokenType.Then,
    'fin': TokenType.End,
}
