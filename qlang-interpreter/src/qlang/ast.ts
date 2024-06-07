export type NodeType =
    // Statements
    | 'Program'
    | 'VariableDeclarationStatement'
    | 'PrintStatement'
    | 'IfStatement'
    | 'WhileStatement'
    | 'ForStatement'
    | 'FunctionStatement'
    | 'BlockStatement'
    | 'BreakStatement'
    | 'ContinueStatement'
    | 'ReturnStatement'
    // Expressions
    | 'AssignmentExpression'
    | 'UnaryExpression'
    | 'BinaryExpression'
    | 'NumericLiteral'
    | 'StringLiteral'
    | 'NullLiteral'
    | 'BooleanLiteral'
    | 'Identifier'
    | 'MemberExpression'
    | 'ArrayExpression'
    | 'CallExpression'

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

export interface WhileStatement extends Statement {
    kind: 'WhileStatement'
    condition: Expression
    body: Statement
}

export interface ForStatement extends Statement {
    kind: 'ForStatement'
    identifier: string
    from: Expression
    until: Expression
    step: Expression
    body: Statement
}

export interface FunctionStatement extends Statement {
    kind: 'FunctionStatement'
    identifier: string | null
    parameters: string[]
    body: BlockStatement
}

export interface BlockStatement extends Statement {
    kind: 'BlockStatement'
    body: Statement[]
}

export interface BreakStatement extends Statement {
    kind: 'BreakStatement'
}

export interface ContinueStatement extends Statement {
    kind: 'ContinueStatement'
}

export interface ReturnStatement extends Statement {
    kind: 'ReturnStatement'
    value: Expression
}

export interface Expression extends Statement { }

export interface AssignmentExpression extends Expression {
    kind: 'AssignmentExpression'
    assignment: Expression
    value: Expression
}

export interface UnaryExpression extends Expression {
    kind: 'UnaryExpression'
    operator: string
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

export interface MemberExpression extends Expression {
    kind: 'MemberExpression'
    object: Expression
    property: Expression | null
}

export interface ArrayExpression extends Expression {
    kind: 'ArrayExpression'
    elements: Expression[]
}

export interface CallExpression extends Expression {
    kind: 'CallExpression'
    callee: Expression
    arguments: Expression[] | FunctionStatement
}

export interface Literal {
    kind: NodeType
}

export interface NumericLiteral extends Literal {
    kind: 'NumericLiteral'
    value: number
}

export interface StringLiteral extends Literal {
    kind: 'StringLiteral'
    value: string
}

export interface NullLiteral extends Literal {
    kind: 'NullLiteral'
    value: 'null'
}

export interface BooleanLiteral extends Literal {
    kind: 'BooleanLiteral'
    value: boolean
}
