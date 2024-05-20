import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

export default function OutputTile() {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('output.title')}>
            Output tile
        </Tile>
    )
}
