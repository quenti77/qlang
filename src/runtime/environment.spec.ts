import { expect, test, describe, beforeEach } from "bun:test"

import Environment from "./environment"
import type { NumberValue } from "./values"

describe("Interpreter", () => {

    let env: Environment

    beforeEach(() => {
        env = new Environment()
    })

    test('declare 2 variables with different names', () => {
        env.declareVariable('a', { type: 'number', value: 42 } as NumberValue)
        env.declareVariable('b', { type: 'number', value: 43 } as NumberValue)

        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
        expect(env.lookupVariable('b')).toEqual({ type: 'number', value: 43 } as NumberValue)
    })

    test('declare 2 variables with the same name', () => {
        env.declareVariable('a', { type: 'number', value: 42 } as NumberValue)

        expect(() => {
            env.declareVariable('a', { type: 'number', value: 43 } as NumberValue)
        }).toThrow("Variable 'a' already declared")
    })

    test('assign a value to a variable that is not declared', () => {
        expect(() => {
            env.assignVariable('a', { type: 'number', value: 42 } as NumberValue)
        }).toThrow("Variable 'a' not declared")
    })

    test('assign a value to a variable that is declared', () => {
        env.declareVariable('a', { type: 'number', value: 42 } as NumberValue)
        env.assignVariable('a', { type: 'number', value: 43 } as NumberValue)

        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 43 } as NumberValue)
    })

    test('lookup a variable that is not declared', () => {
        expect(() => {
            env.lookupVariable('a')
        }).toThrow("Variable 'a' not declared")
    })

    test('lookup a variable that is declared', () => {
        env.declareVariable('a', { type: 'number', value: 42 } as NumberValue)

        expect(env.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

    test('lookup a variable that is declared in a parent environment', () => {
        env.declareVariable('a', { type: 'number', value: 42 } as NumberValue)

        const childEnv = new Environment(env)

        expect(childEnv.lookupVariable('a')).toEqual({ type: 'number', value: 42 } as NumberValue)
    })

})
