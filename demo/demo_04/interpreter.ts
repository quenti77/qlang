// null, boolean, number, string, array, ...

import type {
    BinaryExpression,
    NumericLiteral,
    Program,
    Statement,
    UnaryExpression,
} from "./ast"
import { BINARY_OPERATOR } from "./token"

type ValueType = 'number' | 'null'

interface RuntimeValue {
    type: ValueType
}

interface NumberValue extends RuntimeValue {
    type: 'number'
    value: number
}

function MK_NUMBER(value: number): NumberValue {
    return { type: 'number', value }
}

interface NullValue extends RuntimeValue {
    type: 'null'
    value: null
}

function MK_NULL(): NullValue {
    return { type: 'null', value: null }
}

export default class Interpreter {

    public evaluate(astNode: Statement): RuntimeValue {
        switch (astNode.kind) {
            case "Program":
                return this.evaluateProgram(astNode as Program)
            case "UnaryExpression":
                return this.evaluateUnaryExpression(astNode as UnaryExpression)
            case "BinaryExpression":
                return this.evaluateBinaryExpression(astNode as BinaryExpression)
            case "NumericLiteral":
                return MK_NUMBER((astNode as NumericLiteral).value)
            default:
                return MK_NULL()
        }
    }

    private evaluateProgram(program: Program): RuntimeValue {
        let result: RuntimeValue = MK_NULL()
        for (const statement of program.body) {
            result = this.evaluate(statement)
        }
        return result
    }

    private evaluateUnaryExpression(node: UnaryExpression): RuntimeValue {
        const argument = this.evaluate(node.value)

        if (!('value' in argument)) {
            return MK_NULL()
        }

        if (node.operator === '-') {
            return MK_NUMBER(-(argument.value as number))
        }

        throw new Error(`Unsupported unary operator: ${node.operator}`)
    }

    private evaluateBinaryExpression(node: BinaryExpression): RuntimeValue {
        const left = this.evaluate(node.left)
        const right = this.evaluate(node.right)

        if (!('value' in left) || !('value' in right)) {
            return MK_NULL()
        }

        const leftValue = left.value as number
        const rightValue = right.value as number

        if (node.operator === BINARY_OPERATOR.PLUS) {
            return MK_NUMBER(leftValue + rightValue)
        } else if (node.operator === BINARY_OPERATOR.MINUS) {
            return MK_NUMBER(leftValue - rightValue)
        } else if (node.operator === BINARY_OPERATOR.MULTIPLY) {
            return MK_NUMBER(leftValue * rightValue)
        } else if (node.operator === BINARY_OPERATOR.DIVIDE) {
            return MK_NUMBER(leftValue / rightValue)
        } else if (node.operator === BINARY_OPERATOR.MODULUS) {
            return MK_NUMBER(leftValue % rightValue)
        }

        throw new Error(`Unsupported binary operator: ${node.operator}`)
    }
}
