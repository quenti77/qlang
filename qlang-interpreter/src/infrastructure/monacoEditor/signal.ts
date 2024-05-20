import { editor as MonacoEditor } from "monaco-editor/esm/vs/editor/editor.api"
import { signal } from "@maverick-js/signals"

export default class CodeEditor {
    private el: HTMLElement
    private editor: MonacoEditor.IStandaloneCodeEditor | null = null

    private value: string = ""
    private theme: "light" | "dark" = "light"

    public get Current() { return this.editor }

    public constructor(el: HTMLElement) {
        this.el = el
    }

    public init(initialValue: string = "") {
        this.value = initialValue
        if (this.editor === null) {
            this.editor = MonacoEditor.create(this.el, this.getOptions())
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
            language: "",
            theme: this.theme === "light" ? "vs-light" : "vs-dark",
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            automaticLayout: true,
            fontLigatures: true,
            fontSize: 16,
            fontFamily: "JetBrains Mono, monospace",
            lineHeight: 24,
            // https://github.com/microsoft/monaco-editor/issues/2273
            quickSuggestions: { other: true, strings: true },
        }
    }

}
