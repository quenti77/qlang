export type ValueType = 'null' | 'number' | 'boolean' | 'string'

export interface RuntimeValue {
    type: ValueType
}

export interface AlgebraicValue extends RuntimeValue {
    value: number | string | boolean | null
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
