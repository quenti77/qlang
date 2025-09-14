import type {
    ArrayExpression,
    AssignmentExpression,
    BinaryExpression,
    BlockStatement,
    BooleanLiteral,
    CallExpression,
    ForStatement,
    FunctionStatement,
    Identifier,
    IfStatement,
    MemberExpression,
    NumericLiteral,
    PrintStatement,
    Program,
    ReadExpression,
    ReturnStatement,
    Statement,
    StringLiteral,
    UnaryExpression,
    VariableDeclarationStatement,
    WhileStatement,
} from '../ast'
import {
    type AlgebraicValue,
    ArrayValue,
    FunctionValue,
    MK_ALGEBRAIC,
    MK_BOOLEAN,
    MK_BREAK,
    MK_CONTINUE,
    MK_FUNCTION,
    MK_NULL,
    MK_NUMBER,
    MK_RETURN,
    MK_STRING,
    NumberValue,
    type RuntimeValue,
} from './values'
import Environment from './environment'
import { Std } from './std'
import { QFunction } from './callable'

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
                return this.evaluateBlockStatement(astNode as BlockStatement, this.env)
            case 'IfStatement':
                return this.evaluateIfStatement(astNode as IfStatement)
            case 'WhileStatement':
                return this.evaluateWhileStatement(astNode as WhileStatement)
            case 'ForStatement':
                return this.evaluateForStatement(astNode as ForStatement)
            case 'FunctionStatement':
                return this.evaluateFunctionDeclaration(astNode as FunctionStatement)
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

    public evaluateBlockStatement(blockStatement: BlockStatement, environment: Environment): RuntimeValue {
        const previousEnv = this.env
        this.env = environment

        let lastEvaluated: RuntimeValue = MK_NULL()

        for (const statement of blockStatement.body) {
            if (statement.kind === 'BreakStatement') {
                this.env = previousEnv
                return MK_BREAK()
            }
            if (statement.kind === 'ContinueStatement') {
                this.env = previousEnv
                return MK_CONTINUE()
            }
            if (statement.kind === 'ReturnStatement') {
                const result = this.evaluate((statement as ReturnStatement).value) as AlgebraicValue
                this.env = previousEnv
                return MK_RETURN(result)
            }
            lastEvaluated = this.evaluate(statement)
            if (lastEvaluated.type === 'break' || lastEvaluated.type === 'continue' || lastEvaluated.type === 'return') {
                this.env = previousEnv
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

    private evaluateFunctionDeclaration(functionStatement: FunctionStatement): RuntimeValue {
        const qfunction = new QFunction(functionStatement, this.env)
        const name = qfunction.Name

        const envFound = this.env.resolve(name, false)
        if (envFound === null) {
            this.env.declareVariable(name, MK_NULL())
        }
        return this.env.assignVariable(name, MK_FUNCTION(qfunction))
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
            case 'CallExpression':
                return this.evaluateCallExpression(expression as CallExpression)
            case 'ReadExpression':
                return this.evaluateReadExpression(expression as ReadExpression)
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
                // TODO: throw error
                throw new Error()
        }
    }

    private evaluateAssignmentExpression(node: AssignmentExpression): RuntimeValue {
        if (node.assignment.kind === 'Identifier') {
            const identifier = (node.assignment as Identifier).name
            return this.env.assignVariable(identifier, this.evaluate(node.value))
        }

        if (node.assignment.kind === 'MemberExpression') {
            const memberExpression = node.assignment as MemberExpression
            const object = this.evaluateExpression(memberExpression.object) as AlgebraicValue
            if (object.type !== 'array') {
                // TODO: throw error
                throw new Error()
            }

            const array = object as ArrayValue
            if (memberExpression.property === null) {
                const value = this.attemptAlgebraicValue(this.evaluateExpression(node.value))
                array.value.push(value)

                return value
            }

            const property = this.evaluateExpression(memberExpression.property)
            if (property.type !== 'number') {
                // TODO: throw error
                throw new Error()
            }

            const index = (property as NumberValue).value
            if (index < 0 || index >= array.value.length) {
                // TODO: throw error
                throw new Error()
            }

            const value = this.attemptAlgebraicValue(this.evaluateExpression(node.value))
            return array.value[index] = value
        }

        // TODO: throw error
        throw new Error()
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
            // TODO: throw error
            throw new Error()
        }

        const array = object as ArrayValue
        if (memberExpression.property === null) {
            // TODO: throw error
            throw new Error()
        }

        const property = this.evaluateExpression(memberExpression.property)
        if (property.type !== 'number') {
            // TODO: throw error
            throw new Error()
        }

        const index = (property as NumberValue).value
        if (index < 0 || index >= array.value.length) {
            // TODO: throw error
            throw new Error()
        }

        return array.value[index]
    }

    private evaluateCallExpression(callExpression: CallExpression): RuntimeValue {
        const expression = this.evaluateExpression(callExpression.callee)
        if (expression.type !== 'function') {
            // TODO: throw error
            throw new Error()
        }

        const callee = expression as FunctionValue
        if (callExpression.arguments.length !== callee.value.Arity) {
            // TODO: throw error
            throw new Error()
        }

        const args = callExpression.arguments.map(
            (argument) => this.attemptAlgebraicValue(this.evaluate(argument)),
        )

        const currentValue = callee.value.call(this, args)
        return currentValue.type === 'return'
            ? MK_ALGEBRAIC(currentValue.value)
            : currentValue
    }

    private evaluateReadExpression(readExpr: ReadExpression): RuntimeValue {
        const algebraicValue = this.evaluate(readExpr.value) as AlgebraicValue
        const message = algebraicValue?.value
        const result = prompt(message?.toString() ?? 'Input')

        return result === null ? MK_NULL() : MK_STRING(result)
    }

    private evaluateIdentifier(identifier: Identifier): RuntimeValue {
        return this.env.lookupVariable(identifier.name)
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

        // TODO: throw error
        throw new Error()
    }

    private evaluateAlgebraicBinaryExpression(
        operator: string,
        left: AlgebraicValue,
        right: AlgebraicValue,
    ): AlgebraicValue {
        if ((left.type === 'string' || right.type === 'string') && operator !== '+') {
            // TODO: throw error
            throw new Error()
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

        // TODO: throw error
        throw new Error()
    }

    private attemptAlgebraicValue(value: RuntimeValue): AlgebraicValue {
        if ('value' in value) {
            return value as AlgebraicValue
        }

        // TODO: throw error
        throw new Error()
    }
}
