import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

export default function CodeTile() {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('code.title')}>
            Code tile
        </Tile>
    )
}
