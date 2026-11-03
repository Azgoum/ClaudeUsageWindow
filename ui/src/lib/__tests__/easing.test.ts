import { describe, expect, it } from 'vitest'
import { SPRING, linearEasing, springEase } from '../easing'

describe('linearEasing', () => {
  it('interpolates evenly spaced points', () => {
    const f = linearEasing('linear(0, 0.5, 1)')
    expect(f(0)).toBe(0)
    expect(f(0.25)).toBeCloseTo(0.25)
    expect(f(0.5)).toBeCloseTo(0.5)
    expect(f(1)).toBe(1)
  })
  it('honours explicit stops and spreads the unpositioned ones between them', () => {
    const f = linearEasing('linear(0, 0.2, 1 80%, 1)')
    expect(f(0.4)).toBeCloseTo(0.2) // 0.2 sits halfway between 0% and 80%
    expect(f(0.6)).toBeCloseTo(0.6)
    expect(f(0.9)).toBeCloseTo(1)
  })
  it('clamps the input to [0, 1]', () => {
    const f = linearEasing('linear(0, 1)')
    expect(f(-1)).toBe(0)
    expect(f(2)).toBe(1)
  })
})

describe('SPRING', () => {
  it('is the prototype constant', () => {
    expect(SPRING.startsWith('linear(0, 0.009, 0.035 2.1%, 0.141')).toBe(true)
    expect(SPRING.endsWith('1.004 83.8%, 1)')).toBe(true)
  })
  it('overshoots then settles at 1', () => {
    expect(springEase(0)).toBe(0)
    expect(springEase(0.021)).toBeCloseTo(0.035)
    expect(springEase(0.243)).toBeCloseTo(1.149)
    expect(springEase(1)).toBe(1)
  })
})
