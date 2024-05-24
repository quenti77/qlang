export enum TokenType {
    Number = 'number',
    String = 'string',
    Identifier = 'identifier',
    BinaryOperator = 'binary_operator',
    Equals = 'equals',
    OpenParenthesis = 'open_parenthesis',
    CloseParenthesis = 'close_parenthesis',
    Let = 'let',
    If = 'if',
    Then = 'then',
    Else = 'else',
    End = 'end',
    Null = 'null',
    Boolean = 'boolean',
    Read = 'read',
    Print = 'print',
    EOF = 'eof',
}

export enum BinaryOperator {
    Plus = '+',
    Minus = '-',
    Multiply = '*',
    Divide = '/',
    Modulus = '%',
    EqualsEquals = '==',
    NotEquals = '!=',
    GreaterThan = '>',
    LessThan = '<',
    GreaterThanOrEqual = '>=',
    LessThanOrEqual = '<=',
    And = 'et',
    Or = 'ou',
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
    'dec': TokenType.Let,
    'si': TokenType.If,
    'alors': TokenType.Then,
    'sinon': TokenType.Else,
    'fin': TokenType.End,
    'rien': TokenType.Null,
    'vrai': TokenType.Boolean,
    'faux': TokenType.Boolean,
    'lire': TokenType.Read,
    'ecrire': TokenType.Print,
    'et': TokenType.BinaryOperator,
    'ou': TokenType.BinaryOperator,
}

export const OPERATORS: string[] = Object.values(BinaryOperator)
