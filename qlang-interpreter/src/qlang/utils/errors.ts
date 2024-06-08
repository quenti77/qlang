import { Position } from './position'

export class QError extends Error {
    public posStart: Position
    public posEnd: Position
    public name: string
    public message: string

    constructor(posStart: Position, posEnd: Position, errorName: string, details: string) {
        super()
        this.posStart = posStart
        this.posEnd = posEnd
        this.name = errorName
        this.message = details
    }

    toString(): string {
        return `${this.name}: ${this.message}`
    }
}

export class IllegalCharError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string) {
        super(posStart, posEnd, 'Charactère non valide', `'${details}'`)
    }
}

export class StringUnterminatedError extends QError {
    constructor(posStart: Position, posEnd: Position) {
        super(posStart, posEnd, 'Chaîne non terminée', 'La chaîne n\'est pas terminée')
    }
}
