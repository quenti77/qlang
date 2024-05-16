import React from 'react'
import ReactDOM from 'react-dom/client'

import { RouterProvider } from 'react-router-dom'
import { router } from './routes'

import './app.css'
import { TolgeeProvider } from '@tolgee/react'
import { tolgee } from '../infrastructure/tolgee'

const appRoot = ReactDOM.createRoot(document.getElementById('app')!)
appRoot.render(
    <React.StrictMode>
        <TolgeeProvider tolgee={tolgee}>
            <RouterProvider router={router} />
        </TolgeeProvider>
    </React.StrictMode>,
)
