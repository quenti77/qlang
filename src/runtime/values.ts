export type ValueType = 'null' | 'number' | 'boolean'

export interface RuntimeValue {
    type: ValueType
}

export interface NullValue extends RuntimeValue {
    type: 'null'
    value: 'null'
}

export function MK_NULL(): NullValue {
    return { type: 'null', value: 'null' }
}

export interface NumberValue extends RuntimeValue {
    type: 'number'
    value: number
}

export function MK_NUMBER(value: number): NumberValue {
    return { type: 'number', value }
}

export interface BooleanValue extends RuntimeValue {
    type: 'boolean'
    value: boolean
}

export function MK_BOOLEAN(value: boolean): BooleanValue {
    return { type: 'boolean', value }
}
