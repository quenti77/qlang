import CodeEditor from '../signal'
import useReactive from '@/presentation/hooks/useReactive'

import { useContext, useEffect, useRef, useState } from 'react'
import { editor as MonacoEditor } from "monaco-editor/esm/vs/editor/editor.api"
import { ThemeContext } from '@/infrastructure/themes/ThemeProvider'

interface EditorProps {
    defaultValue?: string
}

export default function Editor({ defaultValue }: EditorProps) {
    const { theme } = useContext(ThemeContext)
    const editorRef = useRef<HTMLDivElement>(null)

    const [editor, setEditor] = useReactive<CodeEditor | null>(null)
    const [value, setValue] = useState(defaultValue || "")

    useEffect(() => {
        if (editorRef.current && editor && editorRef.current.querySelector('.monaco-editor') === null) {
            editor.init(value)
        }
        if (editorRef.current && !editor && editorRef.current.querySelector('.monaco-editor') === null) {
            const newEditor = new CodeEditor(editorRef.current)
            newEditor.init(value)

            setEditor(newEditor)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [editor])

    useEffect(() => {
        if (editor?.Current) {
            editor.setTheme(theme)
        }
    }, [editor, theme])

    return (
        <div ref={editorRef} className="flex-1 flex justify-stretch content-stretch" style={{ height: '94%' }} />
    )
}
