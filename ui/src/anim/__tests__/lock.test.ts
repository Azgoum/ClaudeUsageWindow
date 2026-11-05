import { describe, expect, it } from 'vitest'
import { lockKeys, release } from '../lock'

describe('ring locks', () => {
  it('locks the affected keys with the batch generation', () => {
    expect(lockKeys({}, ['session'], 3)).toEqual({ session: 3 })
    expect(lockKeys({ weekly: 2 }, ['session'], 3)).toEqual({ weekly: 2, session: 3 })
  })
  it('a newer batch takes over an already locked ring', () => {
    expect(lockKeys({ session: 2 }, ['session', 'weekly'], 5)).toEqual({ session: 5, weekly: 5 })
  })
  it('releases only when the generation matches', () => {
    expect(release({ session: 3, weekly: 3 }, 'session', 3)).toEqual({ weekly: 3 })
  })
  it('a stale release (older batch finishing late) keeps the newer lock', () => {
    const locks = { session: 5 }
    expect(release(locks, 'session', 3)).toBe(locks)
  })
  it('releasing an unlocked key is a no-op', () => {
    const locks = {}
    expect(release(locks, 'weekly', 1)).toBe(locks)
  })
})
