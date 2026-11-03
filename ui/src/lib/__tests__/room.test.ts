import { describe, expect, it } from 'vitest'
import { menuRoom, tipRoom } from '../room'

describe('popup room', () => {
  // Widget 145.78 px tall, ring trigger bottom at 122.78 px.
  it('tooltip: ring bottom + 16 gap + height + 6 padding, beyond the widget', () => {
    expect(tipRoom(122.78, 93, 145.78)).toBe(92)
  })
  it('menu: below the cursor + 6 padding, beyond the widget', () => {
    expect(menuRoom(40, 257, 145.78)).toBe(158)
  })
  it('never negative', () => {
    expect(tipRoom(10, 20, 145.78)).toBe(0)
    expect(menuRoom(0, 50, 145.78)).toBe(0)
  })
})
