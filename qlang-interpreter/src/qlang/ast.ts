export type NodeType =
    // Statements
    | 'Program'
    | 'VariableDeclarationStatement'
    // Expressions
    | 'AssignmentExpression'
    | 'NumericLiteral'
    | 'StringLiteral'
    | 'NullLiteral'
    | 'BooleanLiteral'
    | 'Identifier'
    | 'BinaryExpression'

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
    value?: Expression
}

export interface Expression extends Statement { }

export interface AssignmentExpression extends Expression {
    kind: 'AssignmentExpression'
    assignment: Expression
    value: Expression
}

export interface BinaryExpression extends Expression {
    kind: 'BinaryExpression'
    left: Expression
    right: Expression
    operator: string
}

export interface Identifier extends Expression {
    kind: 'Identifier'
    name: string
}

export interface NumericLiteral extends Expression {
    kind: 'NumericLiteral'
    value: number
}

export interface StringLiteral extends Expression {
    kind: 'StringLiteral'
    value: string
}

export interface NullLiteral extends Expression {
    kind: 'NullLiteral'
    value: 'null'
}

export interface BooleanLiteral extends Expression {
    kind: 'BooleanLiteral'
    value: boolean
}
