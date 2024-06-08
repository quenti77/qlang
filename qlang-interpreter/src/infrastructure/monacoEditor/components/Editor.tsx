import CodeEditor from '../signal'
import useReactive from '@/presentation/hooks/useReactive'

import { useCallback, useContext, useEffect, useRef, useState } from 'react'
import { ThemeContext } from '@/infrastructure/themes/ThemeProvider'

interface EditorProps {
    defaultValue?: string
    contentChangeHandler: (content: string) => void
    onExecute: (newCode: string) => void
}

export default function Editor({ defaultValue, contentChangeHandler, onExecute }: EditorProps) {
    const { theme } = useContext(ThemeContext)
    const editorRef = useRef<HTMLDivElement>(null)

    const [editor, setEditor] = useReactive<CodeEditor | null>(null)
    const [value, setValue] = useState(defaultValue || "")

    const onContentChangeHandler = useCallback((event: Event) => {
        if (event instanceof CustomEvent) {
            setValue(event.detail)
            contentChangeHandler(event.detail)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    const onExecuteHandler = useCallback((event: Event) => {
        if (event instanceof CustomEvent) {
            onExecute(event.detail)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    useEffect(() => {
        if (editorRef.current && editor && editorRef.current.querySelector('.monaco-editor') === null) {
            editor.init(value)
            editor.removeEventListener('contentChange', onContentChangeHandler)
            editor.removeEventListener('execute', onExecuteHandler)
            editor.addEventListener('contentChange', onContentChangeHandler)
            editor.addEventListener('execute', onExecuteHandler)
        }
        if (editorRef.current && !editor && editorRef.current.querySelector('.monaco-editor') === null) {
            const newEditor = new CodeEditor(editorRef.current)
            newEditor.init(value)
            newEditor.setTheme(theme)
            newEditor.addEventListener('contentChange', onContentChangeHandler)
            newEditor.addEventListener('execute', onExecuteHandler)

            setEditor(newEditor)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [editor, onContentChangeHandler, onExecute])

    useEffect(() => {
        if (editor?.Current) {
            editor.setTheme(theme)
        }
    }, [editor, theme])

    return (
        <div ref={editorRef} className="flex-1 flex justify-stretch content-stretch overflow-auto" style={{ height: '100%' }} />
    )
}
