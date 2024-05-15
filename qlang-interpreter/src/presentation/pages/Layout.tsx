import { useEffect } from "react"
import { themeChange } from "theme-change"

export default function Layout() {
    useEffect(() => {
        themeChange(false)
    }, [])

    return <></>
}
