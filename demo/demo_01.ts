import Lexer from './demo_01/lexer'
import { header, loopCommand } from './utils'

header('Demo 01: Lexical Analysis')

void loopCommand((code) => {
    const lexer = new Lexer()
    console.log(JSON.stringify(lexer.tokenize(code), null, 2))
})
