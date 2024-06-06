import type {
    ArrayExpression,
    AssignmentExpression,
    BinaryExpression,
    BlockStatement,
    BooleanLiteral,
    ForStatement,
    Identifier,
    IfStatement,
    MemberExpression,
    NumericLiteral,
    PrintStatement,
    Program,
    ReturnStatement,
    Statement,
    StringLiteral,
    UnaryExpression,
    VariableDeclarationStatement,
    WhileStatement,
} from "../ast"
import {
    ArrayValue,
    BreakValue,
    ContinueValue,
    MK_BOOLEAN,
    MK_NULL,
    MK_NUMBER,
    MK_STRING,
    NumberValue,
    ReturnValue,
    type AlgebraicValue,
    type RuntimeValue,
} from "./values"
import Environment from "./environment"
import { Std } from "./std"

export default class Interpreter {

    private env: Environment
    private stdOut: Std
    private stdErr: Std

    public get StdOut(): Std { return this.stdOut }
    public get StdErr(): Std { return this.stdErr }

    constructor(env: Environment, stdOut: Std, stdErr: Std) {
        this.env = env
        this.stdOut = stdOut
        this.stdErr = stdErr
    }

    public evaluate(astNode: Statement): RuntimeValue {
        switch (astNode.kind) {
            case 'Program':
                return this.evaluateProgram(astNode as Program)
            case 'VariableDeclarationStatement':
                return this.evaluateVariableDeclaration(astNode as VariableDeclarationStatement)
            case 'PrintStatement':
                return this.evaluatePrintStatement(astNode as PrintStatement)
            case 'BlockStatement':
                return this.evaluateBlockStatement(astNode as BlockStatement)
            case 'IfStatement':
                return this.evaluateIfStatement(astNode as IfStatement)
            case 'WhileStatement':
                return this.evaluateWhileStatement(astNode as WhileStatement)
            case 'ForStatement':
                return this.evaluateForStatement(astNode as ForStatement)
            default:
                return this.evaluateExpression(astNode)
        }
    }

    private evaluateProgram(program: Program): RuntimeValue {
        let lastEvaluated: RuntimeValue = MK_NULL()

        for (const statement of program.body) {
            lastEvaluated = this.evaluate(statement)
        }
        return lastEvaluated
    }

    private evaluateVariableDeclaration(variableDeclaration: VariableDeclarationStatement): RuntimeValue {
        if (variableDeclaration.value) {
            const value = this.evaluate(variableDeclaration.value)
            this.env.declareVariable(variableDeclaration.identifier, value)
            return value
        }
        this.env.declareVariable(variableDeclaration.identifier, MK_NULL())
        return MK_NULL()
    }

    private evaluatePrintStatement(printStatement: PrintStatement): RuntimeValue {
        const statement = this.evaluate(printStatement.value)
        this.stdOut.print(this.toString(statement))

        return MK_NULL()
    }

    private toString(statement: RuntimeValue): string {
        if (!('value' in statement)) {
            return statement.type
        }

        if (!Array.isArray(statement.value)) {
            return statement.value?.toString() ?? 'rien'
        }

        return this.toStringArray(statement as ArrayValue)
    }

    private toStringArray(array: ArrayValue): string {
        const elements = array.value.map((element) => this.toString(element))
        return `[${elements.join(', ')}]`
    }

    private evaluateBlockStatement(blockStatement: BlockStatement): RuntimeValue {
        let lastEvaluated: RuntimeValue = MK_NULL()

        for (const statement of blockStatement.body) {
            if (statement.kind === 'BreakStatement') {
                return { type: 'break' } as BreakValue
            }
            if (statement.kind === 'ContinueStatement') {
                return { type: 'continue' } as ContinueValue
            }
            if (statement.kind === 'ReturnStatement') {
                const result = this.evaluate((statement as ReturnStatement).value) as AlgebraicValue
                return {
                    type: 'return',
                    value: result.value,
                } as ReturnValue
            }
            lastEvaluated = this.evaluate(statement)
            if (lastEvaluated.type === 'break' || lastEvaluated.type === 'continue' || lastEvaluated.type === 'return') {
                return lastEvaluated
            }
        }
        return lastEvaluated
    }

    private evaluateIfStatement(ifStatement: IfStatement): RuntimeValue {
        const condition = this.evaluate(ifStatement.condition) as AlgebraicValue

        if (condition.value) {
            this.env = new Environment(this.env)
            const result = this.evaluate(ifStatement.thenBranch)

            this.env = this.env.Parent!
            return result
        }

        if (ifStatement.elseBranch) {
            this.env = new Environment(this.env)
            const result = this.evaluate(ifStatement.elseBranch)

            this.env = this.env.Parent!
            return result
        }

        return MK_NULL()
    }

    private evaluateWhileStatement(whileStatement: WhileStatement): RuntimeValue {
        this.env = new Environment(this.env)
        while ((this.evaluate(whileStatement.condition) as AlgebraicValue).value) {
            const result = this.evaluate(whileStatement.body)
            if (result.type === 'break') {
                break
            }
            if (result.type === 'return') {
                this.env = this.env.Parent!
                return result
            }
        }

        this.env = this.env.Parent!
        return MK_NULL()
    }

    private evaluateForStatement(forStatement: ForStatement): RuntimeValue {
        this.env = new Environment(this.env)
        if (this.env.resolve(forStatement.identifier, false) === null) {
            this.env.declareVariable(forStatement.identifier, MK_NULL())
        }
        this.env.assignVariable(forStatement.identifier, this.evaluate(forStatement.from))

        while ((this.evaluate(forStatement.until) as AlgebraicValue).value) {
            const result = this.evaluate(forStatement.body)
            if (result.type === 'break') {
                break
            }
            if (result.type === 'return') {
                this.env = this.env.Parent!
                return result
            }
            this.evaluate(forStatement.step)
        }
        this.env = this.env.Parent!

        return MK_NULL()
    }

    private evaluateExpression(expression: Statement): RuntimeValue {
        switch (expression.kind) {
            case 'AssignmentExpression':
                return this.evaluateAssignmentExpression(expression as AssignmentExpression)
            case 'UnaryExpression':
                return this.evaluateUnaryExpression(expression as UnaryExpression)
            case 'BinaryExpression':
                return this.evaluateBinaryExpression(expression as BinaryExpression)
            case 'ArrayExpression':
                return this.evaluateArrayExpression(expression as ArrayExpression)
            case 'MemberExpression':
                return this.evaluateMemberExpression(expression as MemberExpression)
            case 'Identifier':
                return this.evaluateIdentifier(expression as Identifier)
            case 'NumericLiteral':
                return MK_NUMBER((expression as NumericLiteral).value)
            case 'StringLiteral':
                return MK_STRING((expression as StringLiteral).value)
            case 'NullLiteral':
                return MK_NULL()
            case 'BooleanLiteral':
                return MK_BOOLEAN((expression as BooleanLiteral).value)
            default:
                throw new Error(`Unknown node type ${expression.kind}`)
        }
    }

    private evaluateAssignmentExpression(node: AssignmentExpression): RuntimeValue {
        if (node.assignment.kind !== 'Identifier') {
            throw new Error('Invalid assignment target')
        }

        const identifier = (node.assignment as Identifier).name
        return this.env.assignVariable(identifier, this.evaluate(node.value))
    }

    private evaluateUnaryExpression(unaryExpr: UnaryExpression): RuntimeValue {
        const argument = this.evaluate(unaryExpr.value)

        if ('value' in argument) {
            if (unaryExpr.operator === '-') {
                return MK_NUMBER(-(argument.value as number))
            } else if (unaryExpr.operator === 'non') {
                return MK_BOOLEAN(!(argument.value as boolean))
            }
        }

        return MK_NULL()
    }

    private evaluateBinaryExpression(binaryExpr: BinaryExpression): RuntimeValue {
        if (this.isLogicalOperator(binaryExpr.operator)) {
            return this.evaluateLogicalBinaryExpression(binaryExpr)
        }

        const leftHandSide = this.evaluate(binaryExpr.left)
        const rightHandSide = this.evaluate(binaryExpr.right)

        return this.evaluateAlgebraicBinaryExpression(
            binaryExpr.operator,
            leftHandSide as AlgebraicValue,
            rightHandSide as AlgebraicValue,
        )
    }

    private isLogicalOperator(operator: string): boolean {
        return ['et', 'ou', '==', '!=', '<', '<=', '>', '>='].includes(operator)
    }

    private evaluateArrayExpression(arrayExpression: ArrayExpression): RuntimeValue {
        return {
            type: 'array',
            value: arrayExpression.elements.map((element) => this.evaluate(element))
        } as ArrayValue
    }

    private evaluateMemberExpression(memberExpression: MemberExpression): RuntimeValue {
        const object = this.evaluateExpression(memberExpression.object)
        if (object.type !== 'array') {
            throw new Error('Access to non-array object')
        }

        const property = this.evaluateExpression(memberExpression.property)
        if (property.type !== 'number') {
            throw new Error('Access to non-numeric index')
        }

        const index = (property as NumberValue).value
        if (index < 0 || index >= (object as ArrayValue).value.length) {
            throw new Error('Index out of bounds')
        }

        return (object as ArrayValue).value[index]
    }

    private evaluateIdentifier(identifier: Identifier): RuntimeValue {
        const val = this.env.lookupVariable(identifier.name)
        return val
    }

    private evaluateLogicalBinaryExpression(binaryExpr: BinaryExpression): AlgebraicValue {
        const leftHandSide = this.evaluate(binaryExpr.left) as AlgebraicValue
        const operator = binaryExpr.operator

        if (operator === 'et') {
            if (leftHandSide.type === 'boolean' && leftHandSide.value === false) {
                return MK_BOOLEAN(false)
            }
            return MK_BOOLEAN((this.evaluate(binaryExpr.right) as AlgebraicValue).value as boolean)
        }
        if (operator === 'ou') {
            if (leftHandSide.type === 'boolean' && leftHandSide.value === true) {
                return MK_BOOLEAN(true)
            }
            return MK_BOOLEAN((this.evaluate(binaryExpr.right) as AlgebraicValue).value as boolean)
        }

        const right = this.evaluate(binaryExpr.right) as AlgebraicValue
        if (operator === '==') {
            return MK_BOOLEAN(leftHandSide.value === right.value)
        }
        if (operator === '!=') {
            return MK_BOOLEAN(leftHandSide.value !== right.value)
        }
        if (operator === '<') {
            return MK_BOOLEAN(leftHandSide.value! < right.value!)
        }
        if (operator === '<=') {
            return MK_BOOLEAN(leftHandSide.value! <= right.value!)
        }
        if (operator === '>') {
            return MK_BOOLEAN(leftHandSide.value! > right.value!)
        }
        if (operator === '>=') {
            return MK_BOOLEAN(leftHandSide.value! >= right.value!)
        }
        throw new Error(`Invalid operator ${operator} for logical expression`)
    }

    private evaluateAlgebraicBinaryExpression(
        operator: string,
        left: AlgebraicValue,
        right: AlgebraicValue,
    ): AlgebraicValue {
        if ((left.type === 'string' || right.type === 'string') && operator !== '+') {
            throw new Error('Invalid operation between string and other type')
        }
        if ((left.type === 'string' || right.type === 'string')) {
            return MK_STRING((left.value as string) + (right.value as string))
        }
        if (operator === '+') {
            return MK_NUMBER((left.value as number) + (right.value as number))
        } else if (operator === '-') {
            return MK_NUMBER((left.value as number) - (right.value as number))
        } else if (operator === '*') {
            return MK_NUMBER((left.value as number) * (right.value as number))
        } else if (operator === '/') {
            return MK_NUMBER((left.value as number) / (right.value as number))
        } else if (operator === '%') {
            return MK_NUMBER((left.value as number) % (right.value as number))
        }
        throw new Error(`Invalid operator ${operator} for algebraic expression`)
    }
}
