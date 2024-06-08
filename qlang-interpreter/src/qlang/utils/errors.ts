import { Position } from './position'

export class QError extends Error {
    public posStart: Position
    public posEnd: Position
    public name: string
    public details: string
    public code: string

    constructor(
        posStart: Position,
        posEnd: Position,
        errorName: string,
        details: string,
        code: string,
    ) {
        super()
        this.posStart = posStart
        this.posEnd = posEnd
        this.name = errorName
        this.details = details
        this.code = code

        if (this.posEnd.content === '') {
            this.posEnd = this.posEnd.copy()
            this.posEnd.content = '<ICI>'
        }
    }

    render(): string[] {
        let result = `${this.name}: ${this.details}\n`
        result += `Sur la line ${this.posEnd.line} et colonne ${this.posEnd.col}\n \n`
        result += this.withArrows()
        return result.split('\n')
    }

    private withArrows(): string {
        return this.code.split('\n')[this.posStart.line - 1] + '\n' +
            ' '.repeat(this.posStart.col - 1) + '^'.repeat(this.posEnd.col - this.posStart.col)
    }
}

export class IllegalCharError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string, code: string) {
        super(posStart, posEnd, 'Charactère non valide', `'${details}'`, code)
    }
}

export class StringUnterminatedError extends QError {
    constructor(posStart: Position, posEnd: Position, code: string) {
        super(posStart, posEnd, 'Chaîne non terminée', 'La chaîne n\'est pas terminée', code)
    }
}

export class InvalidSyntaxError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string, code: string) {
        super(posStart, posEnd, 'Syntaxe invalide', details, code)
    }
}

export class MaximumArgumentError extends QError {
    constructor(posStart: Position, posEnd: Position, details: string, code: string) {
        super(posStart, posEnd, 'Nombre d\'arguments maximum dépassé', details, code)
    }
}
