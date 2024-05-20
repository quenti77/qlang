import CodeEditor from "@/infrastructure/monacoEditor/components/Editor"
import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

export default function CodeTile() {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('code.title')}>
            <CodeEditor />
        </Tile>
    )
}
