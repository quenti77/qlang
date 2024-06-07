import { Callable } from "@/qlang/runtime/callable"
import Environment from "@/qlang/runtime/environment"
import {
    AlgebraicValue,
    ArrayValue,
    FunctionValue,
    MK_NUMBER,
} from "@/qlang/runtime/values"

export const makeGlobalEnv = (): Environment => {
    const globalEnv = new Environment()

    const taille = new Callable(1, 'taille', (...args: AlgebraicValue[]): AlgebraicValue => {
        if (args.length !== 1) {
            throw new Error('La fonction taille prend un seul argument')
        }

        if (args[0].type !== 'array') {
            throw new Error('La fonction taille prend un tableau en argument')
        }

        const array = args[0] as ArrayValue
        return MK_NUMBER(array.value.length)
    })
    globalEnv.declareVariable('taille', {
        type: 'function',
        value: taille
    } as FunctionValue)

    return globalEnv
}
