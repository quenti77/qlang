import Lexer from "./src/lexer"
import Parser from "./src/parser"

function cli() {
    const lexer = new Lexer()
    const parser = new Parser()

    console.log("\nWelcome to qlang!")
    console.log("Type 'exit' to quit the cli\n")

    let isRunning = true
    while (isRunning) {
        const input = prompt("(qlang) >")

        if (input === "exit" || !input) {
            isRunning = false
            continue
        }

        lexer.tokenize(input)
        parser.setTokens(lexer.Tokens)
        const ast = parser.makeAST()

        console.log(JSON.stringify(ast, null, 2))
    }

    console.log("Bye!")
}

cli()
