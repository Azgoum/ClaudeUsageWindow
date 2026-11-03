import { describe, expect, it } from 'vitest'
import { fmtDay, fmtDur, fmtTime, resetLine } from '../format'

describe('fmtDur', () => {
  it('seconds', () => expect(fmtDur(45_000, 'en')).toBe('45 s'))
  it('minutes', () => expect(fmtDur(12 * 60_000, 'en')).toBe('12 min'))
  it('hours with padded minutes', () => {
    expect(fmtDur((2 * 60 + 29) * 60_000, 'en')).toBe('2h29')
    expect(fmtDur((60 + 5) * 60_000, 'fr')).toBe('1h05')
  })
  it('days', () => {
    expect(fmtDur((3 * 24 + 12) * 3_600_000, 'en')).toBe('3d 12h')
    expect(fmtDur((3 * 24 + 12) * 3_600_000, 'fr')).toBe('3 j 12 h')
  })
})

describe('fmtTime / fmtDay', () => {
  const fri = new Date(2026, 10, 6, 23, 59)
  it('24 h time', () => expect(fmtTime(new Date(2026, 10, 3, 14, 0))).toBe('14:00'))
  it('day names', () => {
    expect(fmtDay(fri, 'en')).toBe('Fri 23:59')
    expect(fmtDay(fri, 'fr')).toBe('ven. 23:59')
  })
})

describe('resetLine', () => {
  const fri2359 = new Date(2026, 10, 6, 23, 59)
  const tue1130 = new Date(2026, 10, 3, 11, 30)
  it('weekly far away uses day form', () => {
    expect(resetLine('weekly', fri2359, tue1130, 'fr')).toBe('reset ven. 23:59')
    expect(resetLine('weekly', fri2359, tue1130, 'en')).toBe('resets Fri 23:59')
  })
  it('session without reset is idle', () => {
    expect(resetLine('session', null, tue1130, 'en')).toBe('idle')
    expect(resetLine('session', null, tue1130, 'fr')).toBe('inactive')
  })
  it('weekly within 24 h uses time-only form', () => {
    const in20h = new Date(tue1130.getTime() + 20 * 3_600_000)
    expect(resetLine('weekly', in20h, tue1130, 'en')).toBe(`resets ${fmtTime(in20h)}`)
  })
  it('session uses time-only form', () => {
    expect(resetLine('session', new Date(2026, 10, 3, 14, 0), tue1130, 'fr')).toBe('reset à 14:00')
    expect(resetLine('session', new Date(2026, 10, 3, 14, 0), tue1130, 'en')).toBe('resets 14:00')
  })
})
