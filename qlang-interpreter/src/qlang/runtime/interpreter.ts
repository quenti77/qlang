import type {
    AssignmentExpression,
    BinaryExpression,
    BooleanLiteral,
    Identifier,
    NumericLiteral,
    PrintStatement,
    Program,
    Statement,
    StringLiteral,
    VariableDeclarationStatement,
} from "../ast"
import {
    MK_BOOLEAN,
    MK_NULL,
    MK_NUMBER,
    MK_STRING,
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
            case 'AssignmentExpression':
                return this.evaluateAssignmentExpression(astNode as AssignmentExpression)
            case 'PrintStatement':
                return this.evaluatePrintStatement(astNode as PrintStatement)
            case 'BinaryExpression':
                return this.evaluateBinaryExpression(astNode as BinaryExpression)
            case 'Identifier':
                return this.evaluateIdentifier(astNode as Identifier)
            case 'NumericLiteral':
                return MK_NUMBER((astNode as NumericLiteral).value)
            case 'StringLiteral':
                return MK_STRING((astNode as StringLiteral).value)
            case 'NullLiteral':
                return MK_NULL()
            case 'BooleanLiteral':
                return MK_BOOLEAN((astNode as BooleanLiteral).value)
            default:
                throw new Error(`Unknown node type ${astNode.kind}`)
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
        this.stdOut.print('value' in statement ? statement.value?.toString() : statement.type)

        return MK_NULL()
    }

    private evaluateAssignmentExpression(node: AssignmentExpression): RuntimeValue {
        if (node.assignment.kind !== 'Identifier') {
            throw new Error('Invalid assignment target')
        }

        const identifier = (node.assignment as Identifier).name
        return this.env.assignVariable(identifier, this.evaluate(node.value))
    }

    private evaluateBinaryExpression(binaryExpr: BinaryExpression): RuntimeValue {
        const leftHandSide = this.evaluate(binaryExpr.left)
        const rightHandSide = this.evaluate(binaryExpr.right)

        if ('value' in leftHandSide && 'value' in rightHandSide) {
            return this.evaluateAlgebraicBinaryExpression(
                binaryExpr.operator,
                leftHandSide as AlgebraicValue,
                rightHandSide as AlgebraicValue,
            )
        }

        return MK_NULL()
    }

    private evaluateIdentifier(identifier: Identifier): RuntimeValue {
        const val = this.env.lookupVariable(identifier.name)
        return val
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
