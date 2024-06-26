import type { RuntimeValue } from "./interpreter";

export default class Environment {
    private variables: Map<string, RuntimeValue>

    constructor() {
        this.variables = new Map()
    }

    public declareVariable(name: string, value: RuntimeValue): RuntimeValue {
        if (this.variables.has(name)) {
            throw new Error(`Variable '${name}' already declared`)
        }

        this.variables.set(name, value)
        return value
    }

    public lookupVariable(name: string): RuntimeValue {
        if (!this.variables.has(name)) {
            throw new Error(`Variable '${name}' not found`)
        }
        return this.variables.get(name)!
    }
}
