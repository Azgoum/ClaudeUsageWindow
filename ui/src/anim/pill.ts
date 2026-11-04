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

/** Pill base font size and the smallest it may shrink to (px). */
export const PILL_FONT = 10
export const PILL_FONT_MIN = 8.5

/**
 * Font size that keeps the pill on one line inside `avail` px, or null if the base size fits.
 * `width` is the pill's measured width at the base size and `chrome` its padding + border,
 * which don't scale with the font. Rounded down to a quarter px, never below the floor.
 */
export function pillFontSize(width: number, avail: number, chrome: number): number | null {
  if (width <= avail) return null
  const fit = (PILL_FONT * (avail - chrome)) / (width - chrome)
  return Math.max(PILL_FONT_MIN, Math.floor(fit * 4) / 4)
}
