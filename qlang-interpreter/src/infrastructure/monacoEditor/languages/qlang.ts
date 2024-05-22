import { root } from "@maverick-js/signals"
import { languages } from "monaco-editor"

export const LANG_ID = "qlang"

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
    keywords: [
        'dec',
        'si',
        'alors',
        'sinon',
        'sinonsi',
        'fin',
        'rien',
        'vrai',
        'faux',
    ],
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
            [/[{}()\[\]]/, "bracket"],
            [/"[^"]*"/, "string"],
            [/'[^']*'/, "string"],
        ],
    }
})
