import Lexer from './demo_02/lexer'
import AstParser from './demo_02/ast_parser'
import { header, loopCommand } from './utils'

header('Demo 02: Abstract Syntax Tree (AST)')

void loopCommand((code) => {
    const lexer = new Lexer()
    const token = lexer.tokenize(code)

    const astParser = new AstParser()
    console.log(JSON.stringify(astParser.parse(token), null, 2))
})
