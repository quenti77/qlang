import React from 'react'
import ReactDOM from 'react-dom/client'

import { RouterProvider } from 'react-router-dom'
import { router } from './routes'

import './app.css'
import { TolgeeProvider } from '@tolgee/react'
import { tolgee } from '../infrastructure/tolgee'
import { ThemeProvider } from '@/infrastructure/themes/ThemeProvider'

const appRoot = ReactDOM.createRoot(document.getElementById('app')!)
appRoot.render(
    <React.StrictMode>
        <TolgeeProvider tolgee={tolgee}>
            <ThemeProvider>
                <RouterProvider router={router} />
            </ThemeProvider>
        </TolgeeProvider>
    </React.StrictMode>,
)
