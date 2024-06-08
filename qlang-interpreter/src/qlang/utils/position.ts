export class Position {

    public index: number
    public line: number
    public col: number
    public content: string

    constructor(index: number, line: number, col: number, content: string) {
        this.index = index
        this.line = line
        this.col = col
        this.content = content
    }

    public get finishCol(): number {
        return this.col + this.content.length
    }

    public advance(newLine: boolean, content: string): Position {
        this.content = content
        this.index += content.length || 1
        this.col += content.length || 1

        if (newLine) {
            this.line++
            this.col = 1
        }

        return this
    }

    public copy(): Position {
        return new Position(this.index, this.line, this.col, this.content)
    }
}

