import { useTranslate } from "@tolgee/react"
import { IconHome } from "../../infrastructure/icons"
import { LangSelector } from "../../infrastructure/tolgee/LangSelector"
import ThemeSwitch from "../../infrastructure/themes/ThemeSwitch"
import { Link, Outlet } from "react-router-dom"

export default function Layout() {
    const { t } = useTranslate('dashboard')

    return (
        <div className="drawer drawer-open">
            <input id="main-drawer" type="checkbox" className="drawer-toggle" />
            <div className="drawer-content">
                <div className="navbar bg-base-100 shadow-sm p-1 min-h-12">
                    <div className="navbar-start">
                        {/* Navbar on the left */}
                    </div>
                    <div className="navbar-center">
                        <span className="text-green-700 dark:text-green-300 text-lg font-bold">{t("title")}</span>
                    </div>
                    <div className="navbar-end">
                        <LangSelector />
                        <ThemeSwitch />
                    </div>
                </div>
                <Outlet />
            </div>
            <div className="drawer-side">
                <label htmlFor="main-drawer" aria-label="close sidebar" className="drawer-overlay"></label>
                <ul className="menu min-h-full bg-base-200 text-base-content p-0 [&_li>*]:rounded-none">
                    <li className="min-h-12 flex justify-center content-center"><a>QL</a></li>
                    <li><Link to={'/'}><IconHome size="md" /></Link></li>
                </ul>
            </div>
        </div>
    )
}
