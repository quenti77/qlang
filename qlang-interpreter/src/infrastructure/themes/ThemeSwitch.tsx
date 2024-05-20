import { useContext } from "react"
import { IconMoon, IconSun } from "../icons"
import { ThemeContext } from "./ThemeProvider"

export default function ThemeSwitch() {
    const { toggleTheme } = useContext(ThemeContext)

    return (
        <>
            <button className="btn btn-sm hidden dark:block" onClick={() => toggleTheme()}>
                <IconSun size="md" />
            </button>
            <button className="btn btn-sm block dark:hidden" onClick={() => toggleTheme()}>
                <IconMoon size="md" />
            </button>
        </>
    )
}
