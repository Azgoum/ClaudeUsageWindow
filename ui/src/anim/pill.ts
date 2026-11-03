import { fmtDur } from '@/lib/format'
import { t } from '@/lib/i18n'
import type { Lang, ResetEvent } from '@/lib/types'

export type PillKind = 'calm' | 'gold'

export interface PillSpec {
  kind: PillKind
  text: string
  holdMs: number
}

// Prototype `chip` holds: 2200 ms after a planned reset, 2800 ms after a surprise.
const HOLD: Record<PillKind, number> = { calm: 2200, gold: 2800 }

/** The single pill shown for one batch of reset events. */
export function pillFor(events: readonly ResetEvent[], lang: Lang): PillSpec | null {
  if (events.length === 0) return null
  const L = t(lang)
  const kind: PillKind = events.some((e) => e.kind === 'surprise') ? 'gold' : 'calm'
  let text: string
  if (events.length > 1) text = kind === 'gold' ? L.chipBoth : L.chipBothPlanned
  else {
    const e = events[0]
    if (e.kind === 'surprise') text = L.surprise(fmtDur(e.earlySecs * 1000, lang))
    else text = e.key === 'session' ? L.chipS : L.chipW
  }
  return { kind, text, holdMs: HOLD[kind] }
}
