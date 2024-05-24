
export class Std {

    private log: string[]

    public get Log(): string[] { return this.log }

    constructor() {
        this.log = []
    }

    public print(value: string | undefined): void {
        this.log.push(value ?? 'rien')
    }

    public clear(): void {
        this.log = []
    }
}
