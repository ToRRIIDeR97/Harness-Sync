import React from 'react'
import ReactDOM from 'react-dom/client'
import '@fontsource/ibm-plex-sans/400.css'
import '@fontsource/ibm-plex-sans/500.css'
import '@fontsource/ibm-plex-sans/600.css'
import '@fontsource/ibm-plex-sans/700.css'
import '@fontsource/ibm-plex-mono/400.css'
import App from './App'
import Tray from './Tray'
import './styles.css'

const isTray = window.location.hash === '#tray'
document.documentElement.dataset.window = isTray ? 'tray' : 'main'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {isTray ? <Tray /> : <App />}
  </React.StrictMode>,
)
