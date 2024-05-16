import { createBrowserRouter } from "react-router-dom"
import Layout from "./pages/Layout"

export const router = createBrowserRouter([
    {
        path: '/',
        element: <Layout />,
        children: [
            {
                path: '/',
                element: (<div className="p-4 text-center">HELLO</div>)
            }
        ]
    }
])
