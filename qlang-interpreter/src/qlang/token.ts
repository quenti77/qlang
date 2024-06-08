import { Position } from './utils/position'

export enum TokenType {
    Number = 'number',
    String = 'string',
    Identifier = 'identifier',
    UnaryOperator = 'unary_operator',
    BinaryOperator = 'binary_operator',
    Equals = 'equals',
    OpenParenthesis = 'open_parenthesis',
    CloseParenthesis = 'close_parenthesis',
    OpenBrackets = 'open_brackets',
    CloseBrackets = 'close_brackets',
    Let = 'let',
    If = 'if',
    Then = 'then',
    Else = 'else',
    ElseIf = 'else_if',
    End = 'end',
    While = 'while',
    For = 'for',
    From = 'from',
    Until = 'until',
    Step = 'step',
    Return = 'return',
    Break = 'break',
    Continue = 'continue',
    Null = 'null',
    Boolean = 'boolean',
    Read = 'read',
    Print = 'print',
    Function = 'function',
    Comma = 'comma',
    Dot = 'dot',
    EOF = 'eof',
}

export enum UnaryOperator {
    Minus = '-',
    Not = 'non',
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
    position: Position
}

export function createToken(type: TokenType, value: string, position: Position): Token {
    return { type, value, position: position.copy() }
}

export function createTokenAt(type: TokenType, value: string, index: number, line: number, col: number): Token {
    return createToken(type, value, new Position(index, line, col))
}

export const KEYWORDS: Record<string, TokenType> = {
    'dec': TokenType.Let,
    'si': TokenType.If,
    'pour': TokenType.For,
    'de': TokenType.From,
    'jusque': TokenType.Until,
    'evol': TokenType.Step,
    'tantque': TokenType.While,
    'alors': TokenType.Then,
    'sinon': TokenType.Else,
    'sinonsi': TokenType.ElseIf,
    'fin': TokenType.End,
    'arreter': TokenType.Break,
    'continuer': TokenType.Continue,
    'retour': TokenType.Return,
    'rien': TokenType.Null,
    'vrai': TokenType.Boolean,
    'faux': TokenType.Boolean,
    'lire': TokenType.Read,
    'ecrire': TokenType.Print,
    'et': TokenType.BinaryOperator,
    'ou': TokenType.BinaryOperator,
    'non': TokenType.UnaryOperator,
    'fonction': TokenType.Function,
}

export const OPERATORS: string[] = Object.values(BinaryOperator)
