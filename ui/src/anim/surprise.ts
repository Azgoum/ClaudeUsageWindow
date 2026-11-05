import type { AnimationPlaybackControls } from 'motion'
import type { RingHandle } from '@/components/Ring'
import type { SparksApi } from '@/components/Sparks'
import { GOLD } from '@/lib/colors'
import { CIRC, arcOffset } from '@/lib/ring'
import { crossfade, setNumber } from './planned'
import { springEase } from './spring'
import { begin, prefersReducedMotion, run } from './stage'

/** When the pill appears in a surprise reset: flash 380 + number out 260 ms. */
export const SURPRISE_PILL_AT = 640

let glow: AnimationPlaybackControls | undefined

/** Ends the gold glow at once (rings gone: nothing may linger over the connect panel). */
export function stopGlow() {
  glow?.cancel()
  glow = undefined
}

/** The widget's gold glow (an overlay drawn inside the border, see `.wd .glow`), 4.2 s. */
function glowWidget(widgetEl: HTMLElement) {
  const el = widgetEl.querySelector<HTMLElement>('.glow')
  if (!el) return
  glow?.stop()
  glow = run(el, { opacity: [0, 1, 1, 0] }, { duration: 4200, times: [0, 0.12, 0.8, 1] })
}

/**
 * Surprise reset: the arc flashes gold, bursts into sparks and empties, the widget glows gold,
 * the number blurs back in at 0. Port of the prototype's `animSurprise` and `sparks`.
 * The ring must be locked by the caller; it resolves when the ring may show live values again.
 */
export async function playSurprise(
  h: RingHandle,
  widgetEl: HTMLElement,
  fromUsed: number,
  sparks: SparksApi | null,
  onPill?: () => void,
): Promise<void> {
  const { svg, arc, tick, num } = h
  if (!svg || !arc) return
  const st = begin(svg)
  if (prefersReducedMotion()) return crossfade(st, h, onPill)

  const from = arcOffset(fromUsed)
  arc.style.transition = 'none'
  arc.setAttribute('stroke-dashoffset', String(from))
  st.onClear(() => (arc.style.transition = ''))
  arc.setAttribute('stroke', GOLD)
  num?.setAttribute('fill', GOLD)
  void st.play(
    arc,
    { filter: ['brightness(1)', `brightness(1.9) drop-shadow(0 0 6px ${GOLD})`, 'brightness(1.2)'] },
    { duration: 420 },
  )
  glowWidget(widgetEl)
  await st.wait(380)
  if (st.aborted) return

  // Sparks start on the arc, in the widget's padding-box coordinates.
  const wr = widgetEl.getBoundingClientRect()
  const sr = svg.getBoundingClientRect()
  sparks?.burst(
    {
      x: sr.left - wr.left - widgetEl.clientLeft + sr.width / 2,
      y: sr.top - wr.top - widgetEl.clientTop + sr.height / 2,
    },
    (sr.width * 23) / 64,
    Math.max(0.15, fromUsed / 100),
  )
  void st.play(arc, { strokeDashoffset: [from, CIRC], opacity: [1, 0.2] }, { duration: 380, ease: [0.4, 0, 0.2, 1] })
  if (tick) tick.style.opacity = '0'
  await st.play(num, { filter: ['blur(0px)', 'blur(6px)'], opacity: [1, 0] }, { duration: 260 })
  if (st.aborted) return
  setNumber(h, 0)
  void st.play(
    num,
    { filter: ['blur(6px)', 'blur(0px)'], opacity: [0, 1], transform: ['scale(1.4)', 'scale(1)'] },
    { duration: 700, ease: springEase },
  )
  onPill?.()
  await st.wait(2400)
  if (st.aborted) return
  arc.setAttribute('stroke-dashoffset', String(CIRC))
}
