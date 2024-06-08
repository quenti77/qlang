import { Callable, QFunction } from "@/qlang/runtime/callable"
import Environment from "@/qlang/runtime/environment"
import Interpreter from "@/qlang/runtime/interpreter"
import {
    AlgebraicValue,
    FunctionValue,
    MK_NUMBER,
} from "@/qlang/runtime/values"

class TailleFunction implements Callable {

    get Arity(): number {
        return 1
    }

    call(_: Interpreter, args: AlgebraicValue[]): AlgebraicValue {
        if (!Array.isArray(args[0].value)) {
            // TODO: throw error
            throw new Error()
        }

        return MK_NUMBER(args[0].value.length)
    }

    toString() {
        return '<fonction #taille>'
    }
}

export const makeGlobalEnv = (): Environment => {
    const globalEnv = new Environment()
    QFunction.counter = 0

    globalEnv.declareVariable('taille', {
        type: 'function',
        value: new TailleFunction()
    } as FunctionValue)

    return globalEnv
}
