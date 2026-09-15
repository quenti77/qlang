const vscode = require('vscode')
const fs = require('fs')
const path = require('path')

// Mirrors qlang-rs/src/token.rs KEYWORDS.
const KEYWORDS = [
    { word: 'dec', detail: 'Déclare une variable' },
    { word: 'si', detail: 'Bloc conditionnel' },
    { word: 'alors', detail: "Introduit le corps d'un si/tantque/pour" },
    { word: 'sinon', detail: 'Branche alternative du si' },
    { word: 'sinonsi', detail: 'Branche alternative conditionnelle' },
    { word: 'fin', detail: 'Termine un bloc' },
    { word: 'tantque', detail: 'Boucle tant que' },
    { word: 'pour', detail: 'Boucle pour' },
    { word: 'de', detail: "Borne de départ d'une boucle pour" },
    { word: 'jusque', detail: "Condition d'arrêt d'une boucle pour" },
    { word: 'evol', detail: "Pas d'évolution d'une boucle pour" },
    { word: 'retour', detail: "Retourne une valeur d'une fonction" },
    { word: 'arreter', detail: 'Sort de la boucle courante' },
    { word: 'continuer', detail: "Passe à l'itération suivante" },
    { word: 'rien', detail: 'Valeur nulle' },
    { word: 'vrai', detail: 'Booléen vrai' },
    { word: 'faux', detail: 'Booléen faux' },
    { word: 'lire', detail: 'Lit une valeur saisie' },
    { word: 'ecrire', detail: 'Affiche une valeur' },
    { word: 'et', detail: 'Et logique' },
    { word: 'ou', detail: 'Ou logique' },
    { word: 'non', detail: 'Négation logique' },
    { word: 'fonction', detail: 'Déclare une fonction' },
    { word: 'inclure', detail: 'Inclut un fichier module' },
]

const FUNCTION_DECL_RE = /\bfonction\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)/g
const VARIABLE_DECL_RE = /\bdec\s+([A-Za-z_][A-Za-z0-9_]*)/g
const FOR_VARIABLE_RE = /\bpour\s+([A-Za-z_][A-Za-z0-9_]*)/g
const INCLUDE_RE = /\binclure\s+"([^"]+)"/g

function stripComments(text) {
    return text.replace(/\brem\b.*$/gm, '')
}

function collectSymbols(rawText, baseDir, visited, symbols) {
    const text = stripComments(rawText)
    let match

    FUNCTION_DECL_RE.lastIndex = 0
    while ((match = FUNCTION_DECL_RE.exec(text)) !== null) {
        const [, name, rawParams] = match
        const params = rawParams
            .split(',')
            .map((p) => p.trim())
            .filter(Boolean)
        symbols.functions.set(name, params)
    }

    VARIABLE_DECL_RE.lastIndex = 0
    while ((match = VARIABLE_DECL_RE.exec(text)) !== null) {
        symbols.variables.add(match[1])
    }

    FOR_VARIABLE_RE.lastIndex = 0
    while ((match = FOR_VARIABLE_RE.exec(text)) !== null) {
        symbols.variables.add(match[1])
    }

    INCLUDE_RE.lastIndex = 0
    while ((match = INCLUDE_RE.exec(text)) !== null) {
        const includePath = path.resolve(baseDir, match[1])
        if (visited.has(includePath)) {
            continue
        }
        visited.add(includePath)

        try {
            const includedText = fs.readFileSync(includePath, 'utf8')
            collectSymbols(includedText, path.dirname(includePath), visited, symbols)
        } catch {
            // Module introuvable ou illisible : on l'ignore simplement pour l'autocomplétion.
        }
    }

    return symbols
}

function keywordCompletions() {
    return KEYWORDS.map(({ word, detail }) => {
        const item = new vscode.CompletionItem(word, vscode.CompletionItemKind.Keyword)
        item.detail = detail
        return item
    })
}

function symbolCompletions(document) {
    const baseDir = path.dirname(document.uri.fsPath)
    const symbols = collectSymbols(document.getText(), baseDir, new Set(), {
        functions: new Map(),
        variables: new Set(),
    })

    const items = []

    for (const [name, params] of symbols.functions) {
        const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Function)
        item.detail = `fonction ${name}(${params.join(', ')})`
        item.insertText = new vscode.SnippetString(
            params.length === 0
                ? `${name}()`
                : `${name}(${params.map((p, i) => `\${${i + 1}:${p}}`).join(', ')})`
        )
        items.push(item)
    }

    for (const name of symbols.variables) {
        items.push(new vscode.CompletionItem(name, vscode.CompletionItemKind.Variable))
    }

    return items
}

function activate(context) {
    const provider = vscode.languages.registerCompletionItemProvider('qlang', {
        provideCompletionItems(document) {
            return [...keywordCompletions(), ...symbolCompletions(document)]
        },
    })

    context.subscriptions.push(provider)
}

function deactivate() {}

module.exports = { activate, deactivate }
