import { describe, expect, it } from 'vitest'
import { PILL_FONT_MIN, pillFontSize } from '../pill'

describe('pillFontSize', () => {
  it('keeps the base size when the pill fits', () => {
    expect(pillFontSize(170, 236, 18)).toBeNull()
    expect(pillFontSize(236, 236, 18)).toBeNull()
  })
  it('shrinks only the text so the whole pill fits, rounded down to a quarter px', () => {
    // text 230 px at 10 px; room for 218 px of text → 9.478… → 9.25
    const fs = pillFontSize(248, 236, 18)!
    expect(fs).toBe(9.25)
    expect((248 - 18) * (fs / 10) + 18).toBeLessThanOrEqual(236)
  })
  it('never goes below the floor', () => {
    expect(pillFontSize(400, 236, 18)).toBe(PILL_FONT_MIN)
  })
})
