export class Position {

    public index: number
    public line: number
    public col: number

    constructor(index: number, line: number, col: number) {
        this.index = index
        this.line = line
        this.col = col
    }

    public advance(newLine: boolean, increment: number = 1): Position {
        this.index += increment
        this.col += increment

        if (newLine) {
            this.line++
            this.col = 1
        }

        return this
    }

    public copy(): Position {
        return new Position(this.index, this.line, this.col)
    }
}

