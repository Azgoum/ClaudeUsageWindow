import { describe, expect, it } from 'vitest'
import { menuShows } from '../menu'
import type { Conn } from '../types'

describe('menuShows', () => {
  it('connected: refresh, organizations and disconnect', () => {
    expect(menuShows({ state: 'connected' })).toEqual({ connectedItems: true })
  })
  it.each<Conn>([
    { state: 'disconnected', timedOut: false },
    { state: 'disconnected', timedOut: true },
    { state: 'waiting' },
    { state: 'needsFirefox' },
    { state: 'expired' },
  ])('%j: only always-on-top, start with Windows and quit', (conn) => {
    expect(menuShows(conn)).toEqual({ connectedItems: false })
  })
})
