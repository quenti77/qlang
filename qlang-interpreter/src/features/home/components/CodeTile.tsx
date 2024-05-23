import CodeEditor from "@/infrastructure/monacoEditor/components/Editor"
import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

interface CodeTileProps {
    defaultValue?: string
    contentChangeHandler: (content: string) => void
}

export default function CodeTile({ defaultValue, contentChangeHandler }: CodeTileProps) {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('code.title')}>
            <CodeEditor defaultValue={defaultValue} contentChangeHandler={contentChangeHandler} />
        </Tile>
    )
}
