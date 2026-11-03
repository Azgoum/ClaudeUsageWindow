import type { LimitKey } from '@/lib/types'

/** Locked rings: key → generation (the resetSeq of the batch that locked it). */
export type Locks = Partial<Record<LimitKey, number>>

/** Locks `keys` for batch `gen`; a newer batch takes over a ring that is still locked. */
export function lockKeys(locks: Locks, keys: readonly LimitKey[], gen: number): Locks {
  const next = { ...locks }
  for (const k of keys) next[k] = gen
  return next
}

/** Unlocks `key` only if batch `gen` still owns it (an older batch finishing late changes nothing). */
export function release(locks: Locks, key: LimitKey, gen: number): Locks {
  if (locks[key] !== gen) return locks
  const next = { ...locks }
  delete next[key]
  return next
}
