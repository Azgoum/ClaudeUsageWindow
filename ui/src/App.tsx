import { useCallback, useEffect, useState } from 'react'
import { Widget } from '@/components/Widget'
import { getPrefs, getState, inTauri, onState, setLanguage } from '@/lib/bridge'
import type { AppState, Lang, Prefs } from '@/lib/types'

export default function App() {
  const [state, setState] = useState<AppState | null>(null)
  const [prefs, setPrefs] = useState<Prefs | null>(null)
  const [lang, setLang] = useState<Lang>('en')

  useEffect(() => {
    let off: (() => void) | undefined
    let cancelled = false
    void onState(setState).then((un) => (cancelled ? un() : (off = un)))
    void getState().then((s) => setState((cur) => cur ?? s))
    void getPrefs().then((p) => {
      setPrefs(p)
      setLang(p.language)
    })
    return () => {
      cancelled = true
      off?.()
    }
  }, [])

  const onLang = useCallback((l: Lang) => {
    setLang(l)
    setPrefs((p) => p && { ...p, language: l })
    void setLanguage(l)
  }, [])
  const onPrefs = useCallback((patch: Partial<Prefs>) => setPrefs((p) => p && { ...p, ...patch }), [])

  if (!state || !prefs) return null
  const widget = <Widget state={state} prefs={prefs} lang={lang} onLang={onLang} onPrefs={onPrefs} />
  // Browser mock only: a desktop-like backdrop so screenshots compare with the prototype.
  // In Tauri the page stays transparent over the acrylic window.
  return inTauri ? widget : <div className="mock-desktop">{widget}</div>
}
