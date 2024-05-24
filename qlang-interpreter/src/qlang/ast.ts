export type NodeType =
    // Statements
    | 'Program'
    | 'VariableDeclarationStatement'
    | 'PrintStatement'
    | 'IfStatement'
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

export interface PrintStatement extends Statement {
    kind: 'PrintStatement'
    value: Expression
}

export interface IfStatement extends Statement {
    kind: 'IfStatement'
    condition: Expression
    thenBranch: Statement
    elseBranch?: Statement
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
