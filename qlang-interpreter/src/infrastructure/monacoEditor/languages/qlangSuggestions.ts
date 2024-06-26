import { KEYWORDS } from "@/qlang/token"
import { languages, IRange } from "monaco-editor"

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
    pour: `pour \${1:identifiant} de \${2:debut} jusque \${3:fin_inclus} alors
    $4
fin`,
}

const suggestions = Object.keys(KEYWORDS).map((label) => {
    if (label in extendedSuggestions) {
        return [label, extendedSuggestions[label as keyof typeof extendedSuggestions]]
    }
    return [label, label]
})

const code = [
    'fonction fibonacci(n)',
    '    si n <= 1 alors',
    '        retour n',
    '    fin',
    '    retour fibonacci(n - 1) + fibonacci(n - 2)',
    'fin',
    'fibonacci(10)',
]
suggestions.push(['test', code.join('\n')])

interface EnvType {
    type: 'function' | 'variable'
    value: string
}

interface FunctionEnvType extends EnvType {
    type: 'function'
    parameters: string[]
}

class EnvStack {
    private data: Set<EnvType> = new Set()
    private parent: EnvStack | null = null

    constructor(parent: EnvStack | null, data: Set<EnvType> = new Set()) {
        this.parent = parent
        this.data = data
    }

    addVariable(name: string) {
        this.data.add({ type: 'variable', value: name })
    }

    addFunction(name: string, parameters: string[] = []) {
        this.data.add({
            type: 'function',
            value: name,
            parameters
        } as FunctionEnvType)
    }

    createStack() {
        return new EnvStack(this, this.data)
    }

    popStack() {
        return this.parent!
    }

    hasParent() {
        return this.parent !== null
    }

    getScopedVariables(): EnvType[] {
        const parent = this.parent
        if (parent) {
            return [...this.data, ...parent.getScopedVariables()]
        }
        return [...this.data]
    }
}

const kindMap = {
    variable: languages.CompletionItemKind.Variable,
    function: languages.CompletionItemKind.Function,
}

export function getSuggestions(wordRange: IRange, content: string): languages.CompletionItem[] {
    const s = suggestions.map(([label, insertText]) => ({
        label,
        insertText,
        kind: languages.CompletionItemKind.Keyword,
        insertTextRules: languages.CompletionItemInsertTextRule.InsertAsSnippet,
        range: wordRange,
        documentation: {
            value: `**${label}**`,
        },
    } as languages.CompletionItem))

    const lines = content.split('\n')
    let envStack = new EnvStack(null)

    lines.forEach((rawLine) => {
        const line = rawLine.trim()

        if (line.startsWith('dec ')) {
            const varMatch = line.match(/dec ([a-zA-Z_]\w*)/)
            if (varMatch) {
                envStack.addVariable(varMatch[1])
            }
        } else if (line.startsWith('fonction ')) {
            const funcMatch = line.match(/fonction ([a-zA-Z_][a-zA-Z0-9_]*)\((.*)\)/)
            if (funcMatch) {
                envStack.addFunction(
                    funcMatch[1],
                    funcMatch[2].split(',').map((p) => p.trim()),
                )
            }
        } else if (line.startsWith('si ') || line.startsWith('tantque')) {
            envStack = new EnvStack(envStack)
        } else if (line.startsWith('sinonsi') || line.startsWith('sinon')) {
            if (!envStack.hasParent()) return
            envStack = envStack.popStack()
            envStack.createStack()
        } else if (line.startsWith('fin')) {
            if (!envStack.hasParent()) return
            envStack = envStack.popStack()
        }
    })

    const variables = envStack.getScopedVariables().map((envType: EnvType) => {
        const suggestion: languages.CompletionItem = {
            label: envType.value,
            kind: kindMap[envType.type],
            insertText: envType.value,
            insertTextRules: languages.CompletionItemInsertTextRule.InsertAsSnippet,
            range: wordRange,
        }

        if (envType.type === 'function') {
            const func = envType as FunctionEnvType
            suggestion.insertText = `${envType.value}(${func.parameters.map((p) => `\${${p}}`).join(', ')})`
            suggestion.detail = `fonction`
            suggestion.documentation = {
                value: `**Fonction ${envType.value}**\n\n${func.parameters.map((p) => `* ${p}`).join('\n')}`,
                isTrusted: true,
            }
        }

        return suggestion
    })

    return [...s, ...variables]
}
