import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"

interface OutputTileProps {
    output: string[]
}

export default function OutputTile({ output }: OutputTileProps) {
    const { t } = useTranslate('dashboard')

    return (
        <Tile title={t('output.title')}>
            {output.map((line, index) => (
                <p key={index} className="text-sm">{line}</p>
            ))}
        </Tile>
    )
}
