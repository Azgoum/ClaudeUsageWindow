import { describe, expect, it } from 'vitest'
import { t } from '../i18n'

describe('i18n', () => {
  it('FR and EN have identical key sets', () => {
    expect(Object.keys(t('fr')).sort()).toEqual(Object.keys(t('en')).sort())
  })
  it('no string mentions Chrome except needFirefoxText', () => {
    for (const lang of ['fr', 'en'] as const) {
      for (const [k, v] of Object.entries(t(lang))) {
        if (k === 'needFirefoxText') continue
        const s = typeof v === 'function' ? v(1 as never, 1 as never) : v
        expect(String(s), `${lang}.${k}`).not.toMatch(/chrome/i)
      }
    }
  })
  it('no-limit labels', () => {
    expect(t('en').noLimit).toBe('No 5 h limit')
    expect(t('fr').noLimit).toBe('Pas de limite 5 h')
    expect(t('en').planUltra).toBe('Ultra plan')
    expect(t('fr').planUltra).toBe('Forfait Ultra')
  })
  it('format helpers keep prototype wording', () => {
    expect(t('en').used(62)).toBe('62% used')
    expect(t('fr').used(62)).toBe('62 % utilisé')
    expect(t('fr').resetsLong('14:00', '2h29')).toBe('Reset à 14:00 (dans 2h29)')
    expect(t('fr').resetsLong('Ven. 23:59', '3 j')).toBe('Reset Ven. 23:59 (dans 3 j)')
  })
})
