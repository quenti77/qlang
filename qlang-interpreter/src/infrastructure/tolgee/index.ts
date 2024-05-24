import { Tolgee, DevTools, FormatSimple, LanguageDetector, LanguageStorage } from "@tolgee/react"

export const tolgee = Tolgee()
    .use(DevTools())
    .use(FormatSimple())
    .use(LanguageDetector())
    .use(LanguageStorage())
    .init({
        availableLanguages: ['en', 'fr-FR', 'ja-JP'],
        defaultLanguage: 'fr-FR',
        ns: ['dashboard'],

        // for development
        apiUrl: import.meta.env.VITE_APP_TOLGEE_API_URL,
        apiKey: import.meta.env.VITE_APP_TOLGEE_API_KEY,

        staticData: {},
    })
