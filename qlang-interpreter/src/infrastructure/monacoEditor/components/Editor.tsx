import { useEffect, useRef, useState } from 'react'
import CodeEditor from '../signal'

interface EditorProps {
    defaultValue?: string
}

export default function Editor({ defaultValue }: EditorProps) {
    const editorRef = useRef<HTMLDivElement>(null)
    const [editor, setEditor] = useState<CodeEditor | null>(null)

    useEffect(() => {
        console.log({ editor, monacoEditor: editor?.Current, ref: editorRef?.current })
        if (editor === null && editorRef.current) {
            const codeEditor = new CodeEditor(editorRef.current, defaultValue)
            setEditor(codeEditor)
            return
        }
        if (editor && editor.Current === null) {
            editor.init()
        }
        return () => {
            if (editor) {
                editor.removeEditor()
                editor.init()
            }
        }
    }, [editor, editor?.Current, defaultValue])

    return (
        <div ref={editorRef} style={{ flex: 1, height: '100%' }} />
    )
}
