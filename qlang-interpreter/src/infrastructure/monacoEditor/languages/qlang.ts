import { languages } from "monaco-editor"
import { KEYWORDS } from "@/qlang/token"
import Lexer from "@/qlang/lexer"
import Parser from "@/qlang/parser"
import Environment from "@/qlang/runtime/environment"
import Interpreter from "@/qlang/runtime/interpreter"
import { Std } from "@/qlang/runtime/std"
import { AlgebraicValue, RuntimeValue } from "@/qlang/runtime/values"

export const LANG_ID = "qlang"

const lexer = new Lexer()
const parser = new Parser()

function isAlgebraicValue(runtimeValue: RuntimeValue): runtimeValue is AlgebraicValue {
    return 'value' in runtimeValue
}

function formatError(error: unknown): string {
    if (!(error instanceof Error)) {
        return 'Unknown error'
    }
    return error.message
}

export const run = (code: string): { out: string[], err: string[] } => {
    lexer.tokenize(code)
    parser.setTokens(lexer.Tokens)

    const stdOut = new Std()
    const stdErr = new Std()

    try {
        const ast = parser.makeAST()
        const env = new Environment()
        const interpreter = new Interpreter(env, stdOut, stdErr)
        const result = interpreter.evaluate(ast)

        if (isAlgebraicValue(result)) {
            stdOut.print(result.value === null ? 'rien' : result.value.toString())
        }
    } catch (error) {
        stdErr.print(formatError(error))
    }

    return { out: stdOut.Log, err: stdErr.Log }
}


languages.register({ id: LANG_ID })

languages.setLanguageConfiguration(LANG_ID, {
    brackets: [
        ["[", "]"],
        ["(", ")"],
    ],
    comments: {
        lineComment: "rem",
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
            [/rem.*/, "comment"],
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
