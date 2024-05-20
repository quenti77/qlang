import { createContext, useEffect, useState } from "react"

type Theme = 'light' | 'dark'

interface ThemeContextType {
    theme: Theme
    toggleTheme: () => void
}

export const ThemeContext = createContext({} as ThemeContextType)

export function ThemeProvider({ children }: { children: React.ReactNode }) {
    const [theme, setTheme] = useState<Theme>('light')

    useEffect(() => {
        const isDark = localStorage.theme === 'dark' || (!('theme' in localStorage) && window.matchMedia('(prefers-color-scheme: dark)').matches)
        setTheme(isDark ? 'dark' : 'light')
    }, [])

    useEffect(() => {
        if (theme === 'dark') {
            document.documentElement.classList.add('dark')
            document.documentElement.dataset.theme = 'dark'
            localStorage.setItem('theme', 'dark')
        } else {
            document.documentElement.classList.remove('dark')
            document.documentElement.dataset.theme = 'light'
            localStorage.setItem('theme', 'light')
        }
    }, [theme])

    const toggleTheme = () => setTheme((prev) => (prev === 'dark' ? 'light' : 'dark'))

    return (
        <ThemeContext.Provider value={{ theme, toggleTheme }}>
            {children}
        </ThemeContext.Provider>
    )
}
