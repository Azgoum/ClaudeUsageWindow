import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { inTauri } from './lib/bridge'

// Tauri-only look (no window blur behind the glass): see `.tauri .wd` in index.css.
document.documentElement.classList.toggle('tauri', inTauri)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
