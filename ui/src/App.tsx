import { useEffect, useState } from 'react'
import { getState, onState } from '@/lib/bridge'
import type { AppState } from '@/lib/types'

// Placeholder page: the real widget is built in Task 12.
export default function App() {
  const [state, setState] = useState<AppState | null>(null)

  useEffect(() => {
    let off: (() => void) | undefined
    let cancelled = false
    void onState(setState).then((un) => (cancelled ? un() : (off = un)))
    void getState().then((s) => setState((cur) => cur ?? s))
    return () => {
      cancelled = true
      off?.()
    }
  }, [])

  return (
    <pre className="p-2 text-[10px] text-neutral-200">{JSON.stringify(state, null, 2)}</pre>
  )
}
