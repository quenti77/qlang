import { Callable } from "./callable"

export type ValueType =
    | 'null'
    | 'number'
    | 'boolean'
    | 'string'
    | 'break'
    | 'continue'
    | 'return'
    | 'array'
    | 'function'

export interface RuntimeValue {
    type: ValueType
}

export interface BreakValue extends RuntimeValue {
    type: 'break'
}

export function MK_BREAK(): BreakValue {
    return { type: 'break' }
}

export interface ContinueValue extends RuntimeValue {
    type: 'continue'
}

export function MK_CONTINUE(): ContinueValue {
    return { type: 'continue' }
}

type AlgebraicType = 'number' | 'string' | 'boolean' | 'null' | 'array' | 'function' | 'return'
type AlgebraicValueType = number | string | boolean | null | AlgebraicValue[] | Callable

export interface AlgebraicValue extends RuntimeValue {
    type: AlgebraicType
    value: number | string | boolean | null | AlgebraicValue[] | Callable
}

export function MK_ALGEBRAIC(value: AlgebraicValueType): AlgebraicValue {
    if (typeof value === 'number') {
        return MK_NUMBER(value)
    }
    if (typeof value === 'string') {
        return MK_STRING(value)
    }
    if (typeof value === 'boolean') {
        return MK_BOOLEAN(value)
    }
    if (value === null) {
        return MK_NULL()
    }
    if (Array.isArray(value)) {
        return MK_ARRAY(value)
    }
    if ('Arity' in value) {
        return MK_FUNCTION(value)
    }
    throw new Error(`Unsupported algebraic value: ${value}`)
}

export interface FunctionValue extends AlgebraicValue {
    type: 'function'
    value: Callable
}

export function MK_FUNCTION(value: Callable): FunctionValue {
    return { type: 'function', value }
}

export interface ReturnValue extends AlgebraicValue {
    type: 'return'
}

export function MK_RETURN(returnValue: AlgebraicValue): ReturnValue {
    return { type: 'return', value: returnValue.value }
}

export interface ArrayValue extends AlgebraicValue {
    type: 'array'
    value: AlgebraicValue[]
}

export function MK_ARRAY(value: AlgebraicValue[]): ArrayValue {
    return { type: 'array', value }
}

export interface NullValue extends AlgebraicValue {
    type: 'null'
    value: null
}

export function MK_NULL(): NullValue {
    return { type: 'null', value: null }
}

export interface NumberValue extends AlgebraicValue {
    type: 'number'
    value: number
}

export function MK_NUMBER(value: number): NumberValue {
    return { type: 'number', value }
}

export interface BooleanValue extends AlgebraicValue {
    type: 'boolean'
    value: boolean
}

export function MK_BOOLEAN(value: boolean): BooleanValue {
    return { type: 'boolean', value }
}

export interface StringValue extends AlgebraicValue {
    type: 'string'
    value: string
}

export function MK_STRING(value: string): StringValue {
    return { type: 'string', value }
}
