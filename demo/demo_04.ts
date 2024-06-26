import Lexer from './demo_04/lexer'
import AstParser from './demo_04/ast_parser'
import { header, loopCommand } from './utils'
import Interpreter from './demo_04/interpreter'
import Environment from './demo_04/environment'

header('Demo 04: Variables')

const lexer = new Lexer()
const astParser = new AstParser()
const env = new Environment()
const interpreter = new Interpreter(env)

void loopCommand((code) => {
    const token = lexer.tokenize(code)
    const ast = astParser.parse(token)
    const result = interpreter.evaluate(ast)

    console.log(result)
})
