import type { RuntimeValue } from "./values"

export default class Environment {

    private parent?: Environment
    private variables: Map<string, RuntimeValue>

    public get Parent(): Environment | undefined {
        return this.parent
    }

    constructor(parentEnv?: Environment) {
        this.parent = parentEnv
        this.variables = new Map()
    }

    public declareVariable(name: string, value: RuntimeValue): RuntimeValue {
        if (this.variables.has(name)) {
            throw new Error(`Variable '${name}' déjà déclarée`)
        }

        this.variables.set(name, value)
        return value
    }

    public assignVariable(name: string, value: RuntimeValue): RuntimeValue {
        const env = this.resolve(name)!
        env.variables.set(name, value)

        return value
    }

    public lookupVariable(name: string): RuntimeValue {
        const env = this.resolve(name)!
        return env.variables.get(name)!
    }

    public resolve(name: string, throwError: boolean = true): Environment | null {
        if (this.variables.has(name)) {
            return this
        }
        if (this.parent) {
            return this.parent.resolve(name, throwError)
        }
        if (throwError) {
            throw new Error(`Variable '${name}' non déclarée`)
        }
        return null
    }
}
