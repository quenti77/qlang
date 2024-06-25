export type NodeType =
    // Statements
    | 'Program'
    | 'VariableDeclarationStatement'
    // Expressions
    | 'BinaryExpression'
    | 'UnaryExpression'
    | 'NumericLiteral'
    | 'Identifier'

export interface Statement {
    kind: NodeType
}

export interface Program extends Statement {
    kind: 'Program'
    body: Statement[]
}

export interface VariableDeclarationStatement extends Statement {
    kind: 'VariableDeclarationStatement'
    identifier: string
    value: Expression | null
}

export interface Expression extends Statement { }

export interface BinaryExpression extends Expression {
    kind: 'BinaryExpression'
    operator: string
    left: Expression
    right: Expression
}

export interface UnaryExpression extends Expression {
    kind: 'UnaryExpression'
    operator: string
    value: Expression
}

export interface Identifier extends Expression {
    kind: 'Identifier'
    name: string
}

export interface Literal {
    kind: NodeType
}

export interface NumericLiteral extends Literal {
    kind: 'NumericLiteral'
    value: number
}
