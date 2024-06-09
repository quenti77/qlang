import { IconExecute } from "@/infrastructure/icons"
import CodeEditor from "@/infrastructure/monacoEditor/components/Editor"
import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

interface CodeTileProps {
    defaultValue?: string
    contentChangeHandler: (content: string) => void
    onExecute: (newCode?: string) => void
}

function Title({ onExecute }: { onExecute: () => void }) {
    const { t } = useTranslate('dashboard')

    return (
        <div className="flex align-baseline justify-stretch">
            <p className="text-lg font-semibold flex-1">{t('code.title')}</p>
            <button
                className="btn btn-sm bg-amber-400 text-gray-800 hover:bg-amber-500 dark:bg-amber-300 dark:hover:bg-amber-200"
                onClick={() => onExecute()}
            >
                {t('code.execute')}
                <IconExecute size="sm" />
            </button>
        </div>
    )
}

export default function CodeTile({ defaultValue, contentChangeHandler, onExecute }: CodeTileProps) {
    return (
        <Tile title={<Title onExecute={onExecute} />}>
            <CodeEditor
                defaultValue={defaultValue}
                contentChangeHandler={contentChangeHandler}
                onExecute={onExecute}
            />
        </Tile>
    )
}
