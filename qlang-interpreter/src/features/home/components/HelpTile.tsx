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
dec prenom = "Marc"`, { language: "qlang" }).value

const simpleCondition = hljs.highlight(`rem Condition simple
si condition alors
    rem Code à exécuter si la condition est vraie
sinon
    rem Code à exécuter si la condition est fausse
fin`, { language: "qlang" }).value

const simpleWhile = hljs.highlight(`rem Boucle tantque
tantque condition alors
    rem Code à exécuter
fin`, { language: "qlang" }).value

const simpleFor = hljs.highlight(`rem Boucle pour
pour i de 1 à 10 evol 2 faire rem evol est optionnel
    rem Code à exécuter
fin`, { language: "qlang" }).value

const simpleBreak = hljs.highlight(`rem Sortie de boucle
pour i de 1 à 10 alors
    si i = 5 alors
        continuer
    fin
fin
tantque vrai faire
    si condition alors
        sortir
    fin
fin`, { language: "qlang" }).value

const codeClass = "my-2 p-2 overflow-x-auto border border-gray-100 dark:border-gray-700"

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
                    <pre className={codeClass}><code dangerouslySetInnerHTML={{ __html: simpleVariableDeclaration }} /></pre>
                </Card>
                <Card title={t('help.condition.title')}>
                    <p dangerouslySetInnerHTML={{ __html: t('help.condition.intro') }} />
                    <pre className={codeClass}><code dangerouslySetInnerHTML={{ __html: simpleCondition }} /></pre>
                </Card>
                <Card title={t('help.loop.title')}>
                    <p dangerouslySetInnerHTML={{ __html: t('help.loop.intro') }} />
                    <p dangerouslySetInnerHTML={{ __html: t('help.loop.while') }} />
                    <pre className={codeClass}><code dangerouslySetInnerHTML={{ __html: simpleWhile }} /></pre>
                    <p dangerouslySetInnerHTML={{ __html: t('help.loop.for') }} />
                    <pre className={codeClass}><code dangerouslySetInnerHTML={{ __html: simpleFor }} /></pre>
                    <p dangerouslySetInnerHTML={{ __html: t('help.loop.break') }} />
                    <pre className={codeClass}><code dangerouslySetInnerHTML={{ __html: simpleBreak }} /></pre>
                </Card>
            </div>
        </Tile>
    )
}
