export enum TokenType {
    Number = 'number',
    Identifier = 'identifier',
    BinaryOperator = 'binary_operator',
    Equals = 'equals',
    OpenParenthesis = 'open_parenthesis',
    CloseParenthesis = 'close_parenthesis',
    If = 'if',
    Then = 'then',
    Else = 'else',
    ElseIf = 'elseif',
    End = 'end',
    Null = 'null',
    EOF = 'eof',
}

export enum BinaryOperator {
    Plus = '+',
    Minus = '-',
    Multiply = '*',
    Divide = '/',
    Modulus = '%',
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
    'sinon': TokenType.Else,
    'sinonsi': TokenType.ElseIf,
    'fin': TokenType.End,
    'rien': TokenType.Null,
}

export const OPERATORS: string[] = Object.values(BinaryOperator)
