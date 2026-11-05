import type { RingHandle } from '@/components/Ring'
import { PACE_COLOR } from '@/lib/colors'
import { CIRC, arcOffset } from '@/lib/ring'
import { springEase } from './spring'
import { begin, prefersReducedMotion, type Stage } from './stage'

const GREEN = PACE_COLOR.ontrack

/** When the pill appears in a planned reset: glint 750 + number out 200 + settle 500 ms. */
export const PLANNED_PILL_AT = 1450

/** Puts the number's new value in place (the prototype writes the text node directly). */
export function setNumber(h: RingHandle, value: number) {
  const n = h.num?.firstChild
  if (n) n.nodeValue = String(value)
}

/** Reduced motion: no sweep, no sparks, just a crossfade of the number to 0 (the pill still shows). */
export async function crossfade(st: Stage, h: RingHandle, onPill?: () => void) {
  const { arc, tick, num } = h
  if (arc) {
    arc.style.transition = 'none'
    arc.setAttribute('stroke-dashoffset', String(CIRC))
    st.onClear(() => (arc.style.transition = ''))
  }
  if (tick) tick.style.opacity = '0'
  await st.play(num, { opacity: [1, 0] }, { duration: 200 })
  if (st.aborted) return
  setNumber(h, 0)
  onPill?.()
  await st.play(num, { opacity: [0, 1] }, { duration: 300 })
}

/**
 * Planned reset, the "P2" light sweep: a glint runs along the arc, the arc empties, the ring
 * squeezes, the number rolls to 0 and a ✓ appears. Port of the prototype's `animPlanned`.
 * The ring must be locked by the caller; it resolves when the ring may show live values again.
 */
export async function playPlanned(h: RingHandle, fromUsed: number, onPill?: () => void): Promise<void> {
  const { svg, arc, glint, tick, num, ok } = h
  if (!svg || !arc) return
  const st = begin(svg)
  if (prefersReducedMotion()) return crossfade(st, h, onPill)

  const from = arcOffset(fromUsed)
  arc.style.transition = 'none'
  arc.setAttribute('stroke-dashoffset', String(from))
  st.onClear(() => {
    arc.style.transition = ''
    glint?.setAttribute('stroke-dashoffset', '9')
  })

  await st.play(
    glint,
    { opacity: [0, 1, 1, 0], strokeDashoffset: [9, -(CIRC - from) + 9] },
    { duration: 750, ease: 'easeInOut', times: [0, 0.15, 0.85, 1] },
  )
  if (st.aborted) return
  void st.play(arc, { strokeDashoffset: [from, CIRC] }, { duration: 450, ease: [0.6, 0, 0.4, 1] })
  if (tick) tick.style.opacity = '0'
  void st.play(svg, { transform: ['scale(1)', 'scale(.9)', 'scale(1)'] }, { duration: 800, ease: springEase, times: [0, 0.35, 1] })
  await st.play(num, { transform: ['translateY(0)', 'translateY(-8px)'], opacity: [1, 0] }, { duration: 200 })
  if (st.aborted) return
  setNumber(h, 0)
  num?.setAttribute('fill', GREEN)
  arc.setAttribute('stroke', GREEN)
  void st.play(num, { transform: ['translateY(8px)', 'translateY(0)'], opacity: [0, 1] }, { duration: 500, ease: springEase })
  // WAAPI spaces the unpositioned 2nd keyframe at .45; transform has no keyframe after it.
  void st.play(
    ok,
    { opacity: [0, 1, 1, 0], transform: ['scale(.3)', 'scale(1)', 'scale(1)', 'scale(1)'] },
    { duration: 3200, ease: 'easeOut', times: [0, 0.45, 0.9, 1] },
  )
  await st.wait(500)
  if (st.aborted) return
  arc.setAttribute('stroke-dashoffset', String(CIRC))
  onPill?.()
  await st.wait(700)
}
