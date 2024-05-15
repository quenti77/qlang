import React from 'react'
import ReactDOM from 'react-dom/client'

import { RouterProvider } from 'react-router-dom'
import { router } from './routes'

import './app.css'

const appRoot = ReactDOM.createRoot(document.getElementById('app')!)
appRoot.render(
    <React.StrictMode>
        <RouterProvider router={router} />
    </React.StrictMode>,
)
