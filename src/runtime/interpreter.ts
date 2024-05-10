import type { BinaryExpression, NumericLiteral, Program, Statement } from "../ast";
import type { NullValue, NumberValue, RuntimeValue } from "./values";

export default class Interpreter {
    public evaluate(astNode: Statement): RuntimeValue {
        switch (astNode.kind) {
            case 'Program':
                return this.evaluateProgram(astNode as Program)
            case 'BinaryExpression':
                return this.evaluateBinaryExpression(astNode as BinaryExpression)
            case 'NumericLiteral':
                return { type: 'number', value: (astNode as NumericLiteral).value } as NumberValue
            case 'NullLiteral':
                return { type: 'null', value: 'null' } as NullValue
            default:
                throw new Error(`Unknown node type ${astNode.kind}`)
        }
    }

    private evaluateProgram(program: Program): RuntimeValue {
        let lastEvaluated: RuntimeValue = { type: 'null', value: 'null' } as NullValue

        for (const statement of program.body) {
            lastEvaluated = this.evaluate(statement)
        }
        return lastEvaluated
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

        return { type: 'null', value: 'null' } as NullValue
    }

    private evaluateNumericBinaryExpression(operator: string, left: NumberValue, right: NumberValue): NumberValue {
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
        return { type: 'number', value: result }
    }
}
