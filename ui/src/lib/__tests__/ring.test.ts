import { describe, expect, it } from 'vitest'
import {
  CIRC,
  agoText,
  arcOffset,
  errLine,
  isNearLimit,
  numFill,
  arcStroke,
  ringNumber,
  tickAngle,
  tickOpacity,
  tipModel,
  tipTitle,
} from '../ring'
import type { OrgSummary, RingView } from '../types'

const now = new Date(2026, 10, 3, 11, 30).getTime()
const H = 3_600_000
const ring = (p: Partial<RingView>): RingView => ({
  key: 'session',
  used: 62,
  resetsAt: new Date(2026, 10, 3, 14, 0).toISOString(),
  expected: 48,
  pace: 'ahead',
  noLimit: false,
  ...p,
})

describe('ring geometry', () => {
  it('circumference of r=23', () => expect(CIRC).toBeCloseTo(2 * Math.PI * 23, 10))
  it('arc offset empties at 0 and fills at 100, clamped', () => {
    expect(arcOffset(0)).toBeCloseTo(CIRC)
    expect(arcOffset(100)).toBeCloseTo(0)
    expect(arcOffset(50)).toBeCloseTo(CIRC / 2)
    expect(arcOffset(140)).toBeCloseTo(0)
    expect(arcOffset(-5)).toBeCloseTo(CIRC)
  })
  it('tick at expected × 3.6°, hidden when unknown', () => {
    expect(tickAngle(50)).toBe(180)
    expect(tickAngle(null)).toBe(0)
    expect(tickOpacity(48)).toBe(0.95)
    expect(tickOpacity(null)).toBe(0)
  })
  it('number is floored, idle number is gray', () => {
    expect(ringNumber(62.9)).toBe(62)
    expect(numFill(ring({}))).toBe('#fb7185')
    expect(numFill(ring({ pace: 'idle', expected: null, resetsAt: null }))).toBe('#8b8b95')
  })
})

describe('isNearLimit', () => {
  it('true when any ring is at or above 90', () => {
    expect(isNearLimit([ring({ used: 90 }), ring({ key: 'weekly', used: 10 })])).toBe(true)
    expect(isNearLimit([ring({ used: 89.9 }), ring({ key: 'weekly', used: 10 })])).toBe(false)
    expect(isNearLimit(null)).toBe(false)
  })
})

describe('agoText / errLine', () => {
  it('ago has a 1 s floor', () => {
    expect(agoText(new Date(now).toISOString(), now, 'en')).toBe('updated 1 s ago')
    expect(agoText(new Date(now - 12_000).toISOString(), now, 'fr')).toBe('mis à jour il y a 12 s')
    expect(agoText(null, now, 'en')).toBeNull()
  })
  it('error line counts down to retryAt', () => {
    expect(errLine(new Date(now + 2 * 60_000).toISOString(), now, 'en')).toBe(
      '⚠ claude.ai not responding · retry in 2 min',
    )
    expect(errLine(new Date(now + 30_000).toISOString(), now, 'fr')).toBe(
      '⚠ claude.ai ne répond pas · nouvel essai dans 30 s',
    )
  })
})

describe('tipModel', () => {
  it('session ahead of pace', () => {
    const m = tipModel(ring({}), now, 'en')
    expect(m.title).toBe('5-hour session')
    expect(m.kind).toBe('active')
    if (m.kind !== 'active') return
    expect(m.usage).toBe('62% used · expected 48%')
    expect(m.pace).toEqual({ text: '14 pts ahead of pace', color: '#fb7185' })
    expect(m.resets).toBe('Resets 14:00 (in 2h30)')
  })
  it('weekly uses the day form and room phrase, FR spacing before %', () => {
    const fri = new Date(2026, 10, 6, 23, 59)
    const m = tipModel(
      ring({ key: 'weekly', used: 34, expected: 45.4, pace: 'room', resetsAt: fri.toISOString() }),
      now,
      'fr',
    )
    expect(m.title).toBe('Hebdomadaire')
    if (m.kind !== 'active') throw new Error('expected active')
    expect(m.usage).toBe('34 % utilisé · attendu 45 %')
    expect(m.pace).toEqual({ text: '11 pts de marge', color: '#7dd3fc' })
    expect(m.resets).toBe('Reset ven. 23:59 (dans 3 j 12 h)')
  })
  it('on track', () => {
    const m = tipModel(ring({ used: 50, expected: 48, pace: 'ontrack' }), now, 'en')
    if (m.kind !== 'active') throw new Error('expected active')
    expect(m.pace).toEqual({ text: 'on track', color: '#34d399' })
  })
  it('expected unknown: no expected part, no pace phrase', () => {
    const m = tipModel(ring({ expected: null, pace: 'ontrack' }), now, 'en')
    if (m.kind !== 'active') throw new Error('expected active')
    expect(m.usage).toBe('62% used')
    expect(m.pace).toBeNull()
  })
  it('idle session variant', () => {
    const m = tipModel(ring({ used: 0, expected: null, pace: 'idle', resetsAt: null }), now, 'en')
    expect(m).toEqual({
      kind: 'idle',
      title: '5-hour session',
      line: 'no session running',
      sub: 'Starts with your next message',
    })
  })
  it('reset in the past clamps the duration at 0', () => {
    const m = tipModel(ring({ resetsAt: new Date(now - H).toISOString() }), now, 'en')
    if (m.kind !== 'active') throw new Error('expected active')
    expect(m.resets).toMatch(/\(in 0 s\)$/)
  })
})

describe('tipTitle', () => {
  const personal: OrgSummary = { uuid: 'p', name: "jane@example.com's Organization", plan: 'max' }
  const team: OrgSummary = { uuid: 't', name: 'Acme Studio', plan: 'team' }
  it('shows the plan only for a single org', () => {
    expect(tipTitle(personal, [personal], 'en')).toBe('Max plan')
    expect(tipTitle({ ...team }, [team], 'fr')).toBe('Forfait Team')
  })
  it('adds the org name when there are several orgs', () => {
    expect(tipTitle(team, [personal, team], 'en')).toBe('Team plan · Acme Studio')
    expect(tipTitle({ ...team, plan: 'enterprise' }, [personal, team], 'fr')).toBe('Forfait Enterprise · Acme Studio')
  })
  it("never repeats a personal org's \"<name>'s Organization\"", () => {
    expect(tipTitle(personal, [personal, team], 'en')).toBe('Max plan')
    expect(tipTitle({ ...personal, plan: 'pro' }, [personal, team], 'fr')).toBe('Forfait Pro')
  })
  it('falls back to the org name when the plan is unknown', () => {
    expect(tipTitle({ ...team, plan: null }, [team], 'en')).toBe('Acme Studio')
    expect(tipTitle({ ...personal, plan: null }, [personal, team], 'fr')).toBe("jane@example.com's Organization")
    // An unexpected value from a newer backend is never shown raw.
    expect(tipTitle({ ...team, plan: 'galaxy' as never }, [team], 'en')).toBe('Acme Studio')
    expect(tipTitle(null, [], 'en')).toBe('')
  })
})

describe('no-limit session ring (Ultra)', () => {
  const nl = ring({ used: 0, resetsAt: null, expected: null, pace: 'idle', noLimit: true })
  it('is greyed, with no pace tick', () => {
    expect(numFill(nl)).toBe('#8b8b95')
    expect(arcStroke(nl)).toBe('rgba(255,255,255,.18)')
    expect(arcStroke(ring({}))).toBe('#fb7185')
    expect(tickOpacity(nl.expected)).toBe(0)
  })
  it('has its own tooltip, not the idle one', () => {
    const m = tipModel(nl, now, 'en')
    expect(m).toEqual({ kind: 'nolimit', title: '5-hour session', line: 'No 5-hour limit on the Ultra plan' })
    expect(tipModel(nl, now, 'fr')).toMatchObject({ kind: 'nolimit', line: "Pas de limite de 5 h sur le forfait Ultra" })
  })
  it('titles the tooltip with the Ultra plan', () => {
    const ultra: OrgSummary = { uuid: 'u', name: "a@b.c's Organization", plan: 'ultra' }
    expect(tipTitle(ultra, [ultra], 'en')).toBe('Ultra plan')
    expect(tipTitle(ultra, [ultra], 'fr')).toBe('Forfait Ultra')
  })
})
