import { describe, expect, it } from 'vitest'
import { pillFor } from '../pill'
import type { ResetEvent } from '@/lib/types'

const plannedS: ResetEvent = { key: 'session', kind: 'planned', fromUsed: 62 }
const plannedW: ResetEvent = { key: 'weekly', kind: 'planned', fromUsed: 34 }
const surpriseS: ResetEvent = { key: 'session', kind: 'surprise', earlySecs: 8040, fromUsed: 62 }
const surpriseW: ResetEvent = { key: 'weekly', kind: 'surprise', earlySecs: 302_400, fromUsed: 34 }

describe('pillFor', () => {
  it('shows nothing without events', () => {
    expect(pillFor([], 'en')).toBeNull()
  })
  it('one planned session reset → calm chipS', () => {
    expect(pillFor([plannedS], 'en')).toEqual({ kind: 'calm', text: '✓ Session reset · fresh 5 hours', holdMs: 2200 })
  })
  it('one planned weekly reset → calm chipW', () => {
    expect(pillFor([plannedW], 'fr')).toEqual({
      kind: 'calm',
      text: '✓ Nouvelle semaine · limite hebdo remise à zéro',
      holdMs: 2200,
    })
  })
  it('one surprise → gold with the early duration', () => {
    expect(pillFor([surpriseS], 'en')).toEqual({ kind: 'gold', text: '★ Surprise reset · 2h14 early', holdMs: 2800 })
    expect(pillFor([surpriseW], 'fr')?.text).toBe('★ Reset surprise · 3 j 12 h en avance')
  })
  it('two resets with any surprise → gold chipBoth', () => {
    const both = { kind: 'gold', text: '★ Surprise reset · both limits', holdMs: 2800 }
    expect(pillFor([surpriseS, surpriseW], 'en')).toEqual(both)
    expect(pillFor([plannedS, surpriseW], 'en')).toEqual(both)
    expect(pillFor([surpriseS, plannedW], 'en')).toEqual(both)
  })
  it('two planned resets → calm chipBothPlanned', () => {
    expect(pillFor([plannedS, plannedW], 'en')).toEqual({ kind: 'calm', text: '✓ Both limits reset', holdMs: 2200 })
  })
})
