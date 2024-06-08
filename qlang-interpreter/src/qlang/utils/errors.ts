import { Position } from './position'

export class QError extends Error {
    public posStart: Position
    public posEnd: Position
    public name: string
    public details: string

    constructor(posStart: Position, posEnd: Position, errorName: string, details: string) {
        super()
        this.posStart = posStart
        this.posEnd = posEnd
        this.name = errorName
        this.details = details
    }

    render(): string[] {
        console.log('QError.render()', { posStart: this.posStart, posEnd: this.posEnd, name: this.name, details: this.details })
        let result = `${this.name}: ${this.details}\n`
        result += `Sur la line ${this.posEnd.line} et colonne ${this.posEnd.col}\n\n`

        return result.split('\n')
    }

    private withArrows(): string {
        return ''
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

export class InvalidSyntaxError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string) {
        super(posStart, posEnd, 'Syntaxe invalide', details)
    }
}

export class MaximumArgumentError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string) {
        super(posStart, posEnd, 'Nombre d\'arguments maximum dépassé', details)
    }
}
