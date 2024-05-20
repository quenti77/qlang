import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

export default function HelpTile() {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('help.title')}>
            Help tile
        </Tile>
    )
}
