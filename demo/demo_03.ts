import Lexer from './demo_03/lexer'
import AstParser from './demo_03/ast_parser'
import { header, loopCommand } from './utils'
import Interpreter from './demo_03/interpreter'

header('Demo 03: Interpreter')

void loopCommand((code) => {
    const lexer = new Lexer()
    const token = lexer.tokenize(code)

    const astParser = new AstParser()
    const ast = astParser.parse(token)

    const interpreter = new Interpreter()
    const result = interpreter.evaluate(ast)

    console.log(result)
})
