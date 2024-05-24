import Lexer from "./qlang-interpreter/src/qlang/lexer"
import Parser from "./qlang-interpreter/src/qlang/parser"
import Environment from "./qlang-interpreter/src/qlang/runtime/environment"
import Interpreter from "./qlang-interpreter/src/qlang/runtime/interpreter"

function cli() {
    const lexer = new Lexer()
    const parser = new Parser()
    const env = new Environment()
    const interpreter = new Interpreter(env)

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
        const results = interpreter.evaluate(ast)
        console.log(results)
        console.log('-'.repeat(20))
    }

    console.log("Bye!")
}

cli()
