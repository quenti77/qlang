import type { AssignmentExpression, BinaryExpression, BooleanLiteral, Identifier, NumericLiteral, Program, Statement, VariableDeclarationStatement } from "../ast"
import { MK_BOOLEAN, MK_NULL, MK_NUMBER, type NullValue, type NumberValue, type RuntimeValue } from "./values"
import Environment from "./environment"

export default class Interpreter {

    private env: Environment

    constructor(env: Environment) {
        this.env = env
    }

    public evaluate(astNode: Statement): RuntimeValue {
        switch (astNode.kind) {
            case 'Program':
                return this.evaluateProgram(astNode as Program)
            case 'VariableDeclarationStatement':
                return this.evaluateVariableDeclaration(astNode as VariableDeclarationStatement)
            case 'AssignmentExpression':
                return this.evaluateAssignmentExpression(astNode as AssignmentExpression)
            case 'BinaryExpression':
                return this.evaluateBinaryExpression(astNode as BinaryExpression)
            case 'Identifier':
                return this.evaluateIdentifier(astNode as Identifier)
            case 'NumericLiteral':
                return MK_NUMBER((astNode as NumericLiteral).value)
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

        if (leftHandSide.type === 'number' && rightHandSide.type === 'number') {
            return this.evaluateNumericBinaryExpression(
                binaryExpr.operator,
                leftHandSide as NumberValue,
                rightHandSide as NumberValue,
            )
        }

        return MK_NULL()
    }

    private evaluateIdentifier(identifier: Identifier): RuntimeValue {
        const val = this.env.lookupVariable(identifier.name)
        return val
    }

    private evaluateNumericBinaryExpression(
        operator: string,
        left: NumberValue,
        right: NumberValue,
    ): NumberValue {
        let result = 0
        if (operator === '+') {
            result = left.value + right.value
        } else if (operator === '-') {
            result = left.value - right.value
        } else if (operator === '*') {
            result = left.value * right.value
        } else if (operator === '/') {
            result = left.value / right.value
        } else if (operator === '%') {
            result = left.value % right.value
        } else {
            throw new Error(`Unknown operator ${operator}`)
        }
        return MK_NUMBER(result)
    }
}
