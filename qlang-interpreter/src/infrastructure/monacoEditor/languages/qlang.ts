import { languages, editor, type Position } from "monaco-editor"
import { KEYWORDS } from "@/qlang/token"
import Lexer from "@/qlang/lexer"
import Parser from "@/qlang/parser"
import Interpreter from "@/qlang/runtime/interpreter"
import { Std } from "@/qlang/runtime/std"
import { makeGlobalEnv } from "./qlangGlobals"
import {
    AlgebraicValue,
    ArrayValue,
    FunctionValue,
    NumberValue,
    RuntimeValue,
    StringValue,
} from "@/qlang/runtime/values"
import ITextModel = editor.ITextModel
import { QError } from "@/qlang/utils/errors"
import { getSuggestions } from "./qlangSuggestions"

export const LANG_ID = "qlang"

const lexer = new Lexer()
const parser = new Parser()

function isAlgebraicValue(runtimeValue: RuntimeValue): runtimeValue is AlgebraicValue {
    return 'value' in runtimeValue
}

function formatError(error: unknown): string | string[] {
    if (!(error instanceof Error)) {
        return 'Erreur inconnue'
    }
    if (!(error instanceof QError)) {
        return error.message
    }
    return error.render()
}

function toStringAlgebraicValue(element: AlgebraicValue): string {
    switch (element.type) {
        case 'number':
            return (element as NumberValue).value.toString()
        case 'string':
            return (element as StringValue).value
        case 'boolean':
            return element.value ? 'vrai' : 'faux'
        case 'null':
            return 'rien'
        case 'array':
            return toStringArrayValue(element as unknown as ArrayValue)
        case 'function':
            return (element as FunctionValue).value.toString()
        default:
            return 'inconnue'
    }
}

function toStringArrayValue(array: ArrayValue): string {
    const elements = array.value.map(toStringAlgebraicValue).join(', ')
    return `[${elements}]`
}

export const run = (code: string): { out: string[], err: string[] } => {
    const stdOut = new Std()
    const stdErr = new Std()

    try {
        lexer.tokenize(code)
        parser.setTokens(lexer.Tokens, code)

        const ast = parser.makeAST()
        const env = makeGlobalEnv()
        const interpreter = new Interpreter(env, stdOut, stdErr)
        const result = interpreter.evaluate(ast)

        if (isAlgebraicValue(result)) {
            stdOut.print(toStringAlgebraicValue(result))
        }
    } catch (error) {
        let errors = formatError(error)
        if (!Array.isArray(errors)) {
            errors = [errors]
        }
        errors.forEach((err) => stdErr.print(err))
    }

    return { out: stdOut.Log, err: stdErr.Log }
}

function registerLanguage() {
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
        indentationRules: {
            increaseIndentPattern: /.*((\b(si|pour|tantque|pour chaque)\b.*\b(alors)\b)|\bfonction.*\(.*\)).*/,
            decreaseIndentPattern: /^\s*\b(sinon|sinonsi|fin)\b.*$/,
            indentNextLinePattern: /.*((\b(sinon|sinonsi)\b.*\b(alors)?\b)|(fonction.*\(.*\))).*/,
        },
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
        },
    })

    languages.registerCompletionItemProvider(LANG_ID, {
        provideCompletionItems: (
            model: ITextModel,
            position: Position,
        ): languages.ProviderResult<languages.CompletionList> => {
            const word = model.getWordUntilPosition(position)
            const range = {
                startLineNumber: 1,
                startColumn: 1,
                endLineNumber: position.lineNumber,
                endColumn: word.endColumn,
            }
            const wordRange = {
                startLineNumber: position.lineNumber,
                endLineNumber: position.lineNumber,
                endColumn: word.endColumn,
                startColumn: word.startColumn,
            }

            const content = model.getValueInRange(range)
            return {
                suggestions: getSuggestions(wordRange, content)
            }
        },
    })

}

if (languages.getLanguages().find((lang) => lang.id === LANG_ID) === undefined) {
    registerLanguage()
}
