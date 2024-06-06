import { languages, editor, type Position } from "monaco-editor"
import { KEYWORDS } from "@/qlang/token"
import Lexer from "@/qlang/lexer"
import Parser from "@/qlang/parser"
import Environment from "@/qlang/runtime/environment"
import Interpreter from "@/qlang/runtime/interpreter"
import { Std } from "@/qlang/runtime/std"
import { AlgebraicValue, ArrayValue, NumberValue, RuntimeValue, StringValue } from "@/qlang/runtime/values"
import ITextModel = editor.ITextModel

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
        default:
            return 'inconnue'
    }
}

function toStringArrayValue(array: ArrayValue): string {
    const elements = array.value.map(toStringAlgebraicValue).join(', ')
    return `[${elements}]`
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
            stdOut.print(toStringAlgebraicValue(result))
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
    indentationRules: {
        increaseIndentPattern: /.*\b(si|pour|tantque|pour chaque)\b.*\b(alors)\b.*/,
        decreaseIndentPattern: /^\s*\b(sinon|sinonsi|fin)\b.*$/,
        indentNextLinePattern: /.*\b(sinon|sinonsi)\b.*\b(alors)?\b.*/,
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

const extendedSuggestions = {
    dec: `dec \${1:nom}`,
    fonction: `fonction \${1:nom}($2)
    $3
fin`,
    ecrire: `ecrire $1`,
    si: `si \${1:condition} alors
    $2
fin`,
    tantque: `tantque \${1:condition} alors
    $2
fin`,
    pour: `pour \${1:indentifiant} de \${2:debut} jusque \${3:fin_inclus} alors
    $4
fin`,
}

const suggestions = Object.keys(KEYWORDS).map((label) => {
    if (label in extendedSuggestions) {
        return [label, extendedSuggestions[label as keyof typeof extendedSuggestions]]
    }
    return [label, label]
})

languages.registerCompletionItemProvider(LANG_ID, {
    provideCompletionItems: (
        model: ITextModel,
        position: Position,
    ): languages.ProviderResult<languages.CompletionList> => {
        const word = model.getWordUntilPosition(position);
        const range = {
            startLineNumber: position.lineNumber,
            endLineNumber: position.lineNumber,
            startColumn: word.startColumn,
            endColumn: word.endColumn,
        };
        return {
            suggestions: suggestions.map(([label, insertText]) => ({
                label,
                insertText,
                kind: languages.CompletionItemKind.Keyword,
                insertTextRules: languages.CompletionItemInsertTextRule.InsertAsSnippet,
                range: range,
            })),
        }
    }
})
