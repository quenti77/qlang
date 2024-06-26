import Tile from "@/presentation/components/Tile"

import { useTranslate } from "@tolgee/react"
import { useEffect, useRef } from "react"

export interface OutputRow {
    type: 'out' | 'err'
    content: string
}

interface OutputTileProps {
    output: OutputRow[]
}

export default function OutputTile({ output }: OutputTileProps) {
    const { t } = useTranslate('dashboard')
    const listRef = useRef<HTMLDivElement>(null)

    useEffect(() => {
        listRef?.current?.lastElementChild?.scrollIntoView()
    }, [output])

    return (
        <Tile title={t('output.title')}>
            <div ref={listRef} className="text-base flex-1 flex flex-col overflow-y-auto w-full p-2" style={{ fontFamily: 'monospace' }}>
                {output.map((line, index) => (
                    <pre key={index} className={line.type === 'err' ? 'text-red-700 dark:text-red-200' : 'text-green-800 dark:text-green-200'}>
                        {line.content}
                    </pre>
                ))}
            </div>
        </Tile>
    )
}
