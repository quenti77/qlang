import { FunctionStatement } from "../ast"
import Environment from "./environment"
import Interpreter from "./interpreter"
import { AlgebraicValue } from "./values"

export type CallableCallback = (interpreter: Interpreter, ...args: AlgebraicValue[]) => AlgebraicValue

export interface Callable {

    get Arity(): number

    call(interpreter: Interpreter, args: AlgebraicValue[]): AlgebraicValue

    toString(): string

}

export class QFunction implements Callable {

    public static counter: number = 0

    private readonly func: FunctionStatement
    private readonly closure: Environment
    private readonly name: string

    constructor(
        func: FunctionStatement,
        closure: Environment
    ) {
        this.func = func
        this.closure = closure

        if (func.identifier === null) {
            QFunction.counter++
            this.name = `anon#${QFunction.counter}`
        } else {
            this.name = func.identifier
        }
    }

    get Arity(): number {
        return this.func.parameters.length
    }

    get Name(): string {
        return this.name
    }

    call(interpreter: Interpreter, args: AlgebraicValue[]): AlgebraicValue {
        const environment = new Environment(this.closure)

        for (let i = 0; i < this.Arity; i++) {
            environment.declareVariable(this.func.parameters[i], args[i])
        }

        return interpreter.evaluateBlockStatement(this.func.body, environment) as AlgebraicValue
    }

    toString() {
        return `<fonction #${this.name}>`
    }
}
