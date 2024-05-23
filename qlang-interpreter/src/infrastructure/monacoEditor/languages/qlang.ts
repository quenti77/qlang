import { languages } from "monaco-editor"
import { KEYWORDS } from "@/qlang/token"
import Lexer from "@/qlang/lexer"
import Parser from "@/qlang/parser"
import Environment from "@/qlang/runtime/environment"
import Interpreter from "@/qlang/runtime/interpreter"

export const LANG_ID = "qlang"

const lexer = new Lexer()
const parser = new Parser()

export const run = (code: string) => {
    lexer.tokenize(code)
    parser.setTokens(lexer.Tokens)

    const ast = parser.makeAST()

    const env = new Environment()
    const interpreter = new Interpreter(env)
    const results = interpreter.evaluate(ast)

    return 'value' in results ? JSON.parse(JSON.stringify(results.value)) : 'rien'
}


languages.register({ id: LANG_ID })

languages.setLanguageConfiguration(LANG_ID, {
    brackets: [
        ["[", "]"],
        ["(", ")"],
    ],
    comments: {
        lineComment: "REM",
    },
    autoClosingPairs: [
        { open: "[", close: "]" },
        { open: "(", close: ")" },
        { open: '"', close: '"' },
    ],
    surroundingPairs: [
        { open: '"', close: '"' },
    ],
})

languages.setMonarchTokensProvider(LANG_ID, {
    keywords: Array.from(Object.keys(KEYWORDS)),
    tokenizer: {
        root: [
            [/REM.*/, "comment"],
            [/[+\-/*<>=%]/, "delimiter"],
            [
                /[a-zA-Z_]\w*/,
                {
                    cases: {
                        "@keywords": { token: "keyword.$0" },
                    },
                },
            ],
            [
                /dec [a-zA-Z_]\w*/,
                {
                    cases: {
                        "@default": "identifier",
                    },
                },
            ],
            [/\d/, "number"],
            [/[ \t\r\n]+/, ""],
            [/[{}()[\]]/, "bracket"],
            [/"[^"]*"/, "string"],
            [/'[^']*'/, "string"],
        ],
    }
})
