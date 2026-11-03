import type { Lang, LimitKey } from './types'

const MIN = 60_000
const H = 60 * MIN
const D = 24 * H

// Deterministic tables (not Intl) so output is stable across machines and tests.
const DAYS: Record<Lang, string[]> = {
  en: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  fr: ['dim.', 'lun.', 'mar.', 'mer.', 'jeu.', 'ven.', 'sam.'],
}

const pad = (n: number) => String(n).padStart(2, '0')

/** Local time, 24 h: "14:00". */
export const fmtTime = (d: Date): string => `${pad(d.getHours())}:${pad(d.getMinutes())}`

/** "Fri 23:59" / "ven. 23:59" (local time zone). */
export const fmtDay = (d: Date, lang: Lang): string => `${DAYS[lang][d.getDay()]} ${fmtTime(d)}`

/** "45 s" / "12 min" / "2h29" / "3d 12h" / "3 j 12 h". */
export function fmtDur(ms: number, lang: Lang): string {
  ms = Math.max(0, ms)
  if (ms < MIN) return `${Math.ceil(ms / 1000)} s`
  if (ms >= D) {
    const d = Math.floor(ms / D)
    const h = Math.floor((ms % D) / H)
    return lang === 'fr' ? `${d} j ${h} h` : `${d}d ${h}h`
  }
  const totalMin = Math.round(ms / MIN) // rounds first so 1h59m40s never prints "1h60"
  const h = Math.floor(totalMin / 60)
  const m = totalMin % 60
  return h ? `${h}h${pad(m)}` : `${m} min`
}

/** Short reset line under a ring. Day form only for weekly limits more than 24 h away. */
export function resetLine(key: LimitKey, resetsAt: Date | null, now: Date, lang: Lang): string {
  if (!resetsAt) return lang === 'fr' ? 'inactive' : 'idle'
  if (key === 'weekly' && resetsAt.getTime() - now.getTime() > D) {
    return `${lang === 'fr' ? 'reset' : 'resets'} ${fmtDay(resetsAt, lang)}`
  }
  return `${lang === 'fr' ? 'reset à' : 'resets'} ${fmtTime(resetsAt)}`
}
