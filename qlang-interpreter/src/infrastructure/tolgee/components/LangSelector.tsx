import { useTolgee } from "@tolgee/react"

export const LangSelector = () => {
    const { getLanguage, changeLanguage } = useTolgee()
    const availableLanguages = {
        "fr-FR": "Français",
        "en": "English",
        "ja-JP": "日本語",
    }

    return (
        <div className="dropdown dropdown-end">
            <div tabIndex={0} role="button" className="btn btn-sm m-1">{availableLanguages[getLanguage() as keyof typeof availableLanguages]}</div>
            <ul tabIndex={0} className="dropdown-content z-[1] menu p-2 shadow bg-base-100 rounded-box w-52">
                {Object.entries(availableLanguages).map(([key, lang]) => (
                    <li key={key} onClick={() => changeLanguage(key)}>
                        <a className={getLanguage() === key ? "active" : ""}>{lang}</a>
                    </li>
                ))}
            </ul>
        </div>
    )
}
