import { ThemeContext } from "@/infrastructure/themes/ThemeProvider"
import Card from "@/presentation/components/Card"
import Tile from "@/presentation/components/Tile"
import hljs from "highlight.js"
import "@/infrastructure/hljs/qlang"

import { useTranslate } from "@tolgee/react"
import { useContext, useEffect } from "react"

const simpleVariableDeclaration = hljs.highlight(`rem Déclaration de variable simple
dec nom
rem Déclaration de variable avec valeur
dec prenom = "Jean"`, { language: "qlang" }).value

export default function HelpTile() {
    const { t } = useTranslate('dashboard')
    const { theme } = useContext(ThemeContext)

    useEffect(() => {
        const nodes = document.querySelectorAll('.qlang')
        nodes.forEach(node => {
            hljs.highlightBlock(node as HTMLElement)
        })
    }, [theme])

    return (
        <Tile title={t('help.title')}>
            <div className="flex-1 flex flex-col overflow-y-auto w-full p-2">
                <Card title={t('help.variable.title')}>
                    <p dangerouslySetInnerHTML={{ __html: t('help.variable.intro') }} />
                    <pre className="my-2"><code dangerouslySetInnerHTML={{ __html: simpleVariableDeclaration }} /></pre>
                </Card>
            </div>
        </Tile >
    )
}
