import { editor as MonacoEditor, MarkerSeverity } from "monaco-editor/esm/vs/editor/editor.api"
import { signal } from "@maverick-js/signals"

export default class CodeEditor {
    private el: HTMLElement
    private value: string = ""
    private editor: MonacoEditor.IStandaloneCodeEditor | null = null

    public get Current() { return this.editor }

    public constructor(el: HTMLElement, initialValue: string = "") {
        this.el = el
        this.value = initialValue
    }

    public init() {
        console.log("init")
        if (this.editor) {
            return
        }

        console.log("init editor")
        this.editor = MonacoEditor.create(this.el, {
            value: this.value,
            language: "",
            theme: "",
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            automaticLayout: true,
            fontLigatures: true,
            fontSize: 18,
            fontFamily: "JetBrains Mono, monospace",
            lineHeight: 24,
            scrollbar: {
                verticalScrollbarSize: 5,
                verticalSliderSize: 3,
                horizontalScrollbarSize: 5,
                horizontalSliderSize: 3,
            },
            // https://github.com/microsoft/monaco-editor/issues/2273
            quickSuggestions: { other: true, strings: true },
        })
    }

    public removeEditor() {
        if (this.editor) {
            this.editor.dispose()
            this.editor = null
        }
    }
}
