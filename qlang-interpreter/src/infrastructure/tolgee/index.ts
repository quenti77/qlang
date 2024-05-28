import { Tolgee, DevTools, FormatSimple, LanguageDetector, LanguageStorage } from "@tolgee/react"
import frDashboard from "./dashboard/fr-FR.json"
import enDashboard from "./dashboard/en.json"
import jaDashboard from "./dashboard/ja-JP.json"

export const tolgee = Tolgee()
    .use(DevTools())
    .use(FormatSimple())
    .use(LanguageDetector())
    .use(LanguageStorage())
    .init({
        availableLanguages: ['en', 'fr-FR', 'ja-JP'],
        defaultLanguage: 'fr-FR',
        ns: ['dashboard'],

        staticData: {
            'fr-FR:dashboard': frDashboard,
            'en:dashboard': enDashboard,
            'ja-JP:dashboard': jaDashboard,
        },
    })
