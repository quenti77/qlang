import { AlgebraicValue } from "./values"

export type CallableCallback = (...args: AlgebraicValue[]) => AlgebraicValue

export class Callable {

    private static counter: number = 0

    public readonly arity: number
    public readonly name: string
    public readonly call: CallableCallback

    constructor(
        arity: number,
        name: string | null,
        call: CallableCallback,
    ) {
        this.arity = arity
        this.call = call

        if (name === null) {
            Callable.counter++
            name = `anon#${Callable.counter}`
        }
        this.name = name
    }

    toString() {
        return `<fonction #${this.name}>`
    }
}
