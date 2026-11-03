import { PACE_COLOR } from './colors'
import { fmtDay, fmtDur, fmtTime } from './format'
import { t } from './i18n'
import type { Lang, RingView } from './types'

/** Circumference of the r=23 ring. */
export const CIRC = 2 * Math.PI * 23

const clamp100 = (v: number) => Math.max(0, Math.min(100, v))

export const arcOffset = (used: number): number => CIRC * (1 - clamp100(used) / 100)
export const tickAngle = (expected: number | null): number => (expected ?? 0) * 3.6
export const tickOpacity = (expected: number | null): number => (expected == null ? 0 : 0.95)
export const ringNumber = (used: number): number => Math.floor(used)
export const numFill = (r: RingView): string => (r.pace === 'idle' ? '#8b8b95' : PACE_COLOR[r.pace])

export const isNearLimit = (rings: readonly RingView[] | null): boolean =>
  !!rings?.some((r) => r.used >= 90)

export const agoText = (lastOk: string | null, now: number, lang: Lang): string | null =>
  lastOk ? t(lang).ago(fmtDur(Math.max(1000, now - Date.parse(lastOk)), lang)) : null

export const errLine = (retryAt: string, now: number, lang: Lang): string =>
  t(lang).err(fmtDur(Date.parse(retryAt) - now, lang))

export type TipModel =
  | { kind: 'idle'; title: string; line: string; sub: string }
  | {
      kind: 'active'
      title: string
      usage: string
      pace: { text: string; color: string } | null
      resets: string | null
    }

/** Text of the ring tooltip (prototype `updTip`), minus the org/updated header. */
export function tipModel(r: RingView, now: number, lang: Lang): TipModel {
  const L = t(lang)
  const title = r.key === 'session' ? L.s : L.w
  if (r.pace === 'idle') return { kind: 'idle', title, line: L.noSession, sub: L.startsNext }
  const exp = r.expected
  const usage = L.used(ringNumber(r.used)) + (exp != null ? ` · ${L.expected(Math.round(exp))}` : '')
  let pace: { text: string; color: string } | null = null
  if (exp != null) {
    const diff = Math.round(r.used - exp)
    const text = r.pace === 'ahead' ? L.ahead(diff) : r.pace === 'room' ? L.room(-diff) : L.onTrack
    pace = { text, color: PACE_COLOR[r.pace] }
  }
  let resets: string | null = null
  if (r.resetsAt) {
    const at = new Date(r.resetsAt)
    const when = r.key === 'session' ? fmtTime(at) : fmtDay(at, lang)
    resets = L.resetsLong(when, fmtDur(at.getTime() - now, lang))
  }
  return { kind: 'active', title, usage, pace, resets }
}
