import { expect, test, describe } from "bun:test"
import { Callable } from "./callable"
import { AlgebraicValue, MK_NULL, MK_NUMBER, NumberValue } from "./values"

describe("Callable", () => {

    test("create a callable with a name", () => {
        const callable = new Callable(0, "test", () => MK_NULL())
        expect(callable.name).toBe("test")
    })

    test("create a callable without a name", () => {
        const callable = new Callable(0, null, () => MK_NULL())
        expect(callable.name).toBe("anon#1")
    })

    test("call the callable", () => {
        const callable = new Callable(0, null, () => MK_NULL())
        expect(callable.call()).toEqual(MK_NULL())
    })

    test("call the callable with arguments", () => {
        const callback = (...args: AlgebraicValue[]) => MK_NUMBER((args[0] as NumberValue).value + (args[1] as NumberValue).value)
        const callable = new Callable(2, null, callback)
        expect(callable.call(MK_NUMBER(1), MK_NUMBER(2))).toEqual(MK_NUMBER(3))
    })

})
