import { editor as MonacoEditor, KeyCode, KeyMod } from "monaco-editor/esm/vs/editor/editor.api"
import { LANG_ID } from "./languages/qlang"

export default class CodeEditor extends EventTarget {
    private el: HTMLElement
    private editor: MonacoEditor.IStandaloneCodeEditor | null = null

    private value: string = ""
    private theme: "light" | "dark" = "light"

    public get Current() { return this.editor }

    public constructor(el: HTMLElement) {
        super()
        this.el = el
    }

    public init(initialValue: string = "") {
        this.value = initialValue
        if (this.editor === null) {
            this.editor = MonacoEditor.create(this.el, this.getOptions())
            this.editor.onDidChangeModelContent(() => {
                this.value = this.editor?.getValue() || ""
                this.dispatchEvent(new CustomEvent('contentChange', { detail: this.value }))
            })
            this.editor.addAction({
                id: 'execute',
                label: 'Execute',
                keybindings: [KeyMod.CtrlCmd | KeyCode.Enter],
                precondition: undefined,
                keybindingContext: undefined,
                contextMenuGroupId: "navigation",
                contextMenuOrder: 1.5,
                run: () => {
                    this.value = this.editor?.getValue() || ""
                    this.dispatchEvent(new CustomEvent('execute', { detail: this.value }))
                },
            })
        } else {
            this.editor.updateOptions(this.getOptions())
        }
    }

    public setTheme(theme: "light" | "dark") {
        this.theme = theme
        if (this.editor) {
            this.editor.updateOptions(this.getOptions())
        }
    }

    private getOptions() {
        return {
            value: this.value,
            language: LANG_ID,
            theme: this.theme === "light" ? "vs-light" : "vs-dark",
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            automaticLayout: true,
            autoIndent: "full",
            formatOnPaste: true,
            formatOnType: true,
            fontLigatures: true,
            fontSize: 16,
            fontFamily: "JetBrains Mono, monospace",
            lineHeight: 32,
            // https://github.com/microsoft/monaco-editor/issues/2273
            quickSuggestions: { other: true, strings: true },
        } as MonacoEditor.IEditorOptions & MonacoEditor.IGlobalEditorOptions
    }

}
