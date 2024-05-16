import { useEffect } from "react"
import { IconMoon, IconSun } from "../icons"

export default function ThemeSwitch() {
    useEffect(() => {
        if (localStorage.theme === 'dark' || (!('theme' in localStorage) && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
            document.documentElement.classList.add('dark')
            document.documentElement.dataset.theme = 'dark'
        } else {
            document.documentElement.classList.remove('dark')
            document.documentElement.dataset.theme = 'light'
        }
    }, [])

    const setTheme = (theme: string) => {
        localStorage.setItem('theme', theme)
        document.documentElement.classList.remove(theme === 'dark' ? 'light' : 'dark')
        document.documentElement.classList.add(theme)
        document.documentElement.dataset.theme = theme
    }

    return (
        <>
            <button className="btn btn-sm hidden dark:block" onClick={() => setTheme('light')}>
                <IconSun size="md" />
            </button>
            <button className="btn btn-sm block dark:hidden" onClick={() => setTheme('dark')}>
                <IconMoon size="md" />
            </button>
        </>
    )
}
