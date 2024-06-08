export class QError extends Error {
    constructor(errorName: string, details: string) {
        super()
        this.name = errorName
        this.message = details
    }

    toString(): string {
        return `${this.name}: ${this.message}`
    }
}

export class IllegalCharError extends QError {
    constructor(details: string) {
        super('Charactère non valide', `'${details}'`)
    }
}

export class StringUnterminatedError extends QError {
    constructor() {
        super('Chaîne non terminée', 'La chaîne n\'est pas terminée')
    }
}
