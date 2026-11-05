import { useEffect, useState } from 'react'

/** Current time in ms, re-rendering every `intervalMs` while `enabled`. */
export function useNow(intervalMs: number, enabled = true): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!enabled) return
    const tick = () => setNow(Date.now())
    const first = setTimeout(tick, 0) // catch up right away: `now` may be stale while disabled
    const id = setInterval(tick, intervalMs)
    return () => {
      clearTimeout(first)
      clearInterval(id)
    }
  }, [intervalMs, enabled])
  return now
}
