import { useImperativeHandle, useRef, type Ref } from 'react'
import type { AnimationPlaybackControls } from 'motion'
import { pillFontSize, type PillKind } from '@/anim/pill'
import { springEase } from '@/anim/spring'
import { prefersReducedMotion, run } from '@/anim/stage'

export interface PillApi {
  show(kind: PillKind, text: string, holdMs: number): void
  /** Ends the pill at once (rings gone: nothing may linger over the connect panel). */
  hide(): void
}

/** Room left on each side of the pill inside the widget (px). */
const SIDE_MARGIN = 4

/**
 * The one reset pill (prototype `chip`). The prototype straddles the top border; here the window
 * is exactly the widget's box, so it sits in the free band above the rings, on one line. Text
 * too wide for the widget (French chipW) shrinks, down to PILL_FONT_MIN, instead of wrapping.
 */
export function Pill({ ref }: { ref?: Ref<PillApi> }) {
  const el = useRef<HTMLDivElement>(null)
  const seq = useRef(0)
  const cur = useRef<AnimationPlaybackControls | undefined>(undefined)
  useImperativeHandle(
    ref,
    () => ({
      show(kind, text, holdMs) {
        const c = el.current
        if (!c) return
        const n = ++seq.current
        cur.current?.cancel()
        c.className = `chip ${kind}`
        c.textContent = text
        // Fit on one line. offsetWidth ignores the transforms, so this works mid-animation.
        c.style.fontSize = ''
        const avail = (c.parentElement?.clientWidth ?? 0) - 2 * SIDE_MARGIN
        const cs = getComputedStyle(c)
        const chrome =
          parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) + parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth)
        const fs = avail > 0 ? pillFontSize(c.offsetWidth, avail, chrome) : null
        if (fs !== null) c.style.fontSize = `${fs}px`
        const still = prefersReducedMotion()
        const play = (a: AnimationPlaybackControls) => {
          cur.current = a
          return a.finished.then(() => n === seq.current, () => false)
        }
        void (async () => {
          const inOk = await play(
            still
              ? run(c, { opacity: [0, 1] }, { duration: 300 })
              : run(
                  c,
                  { opacity: [0, 1], transform: ['translate(-50%,6px) scale(.85)', 'translate(-50%,0px) scale(1)'] },
                  { duration: 650, ease: springEase },
                ),
          )
          if (!inOk) return
          await new Promise((r) => setTimeout(r, holdMs))
          if (n !== seq.current) return
          await play(
            still
              ? run(c, { opacity: [1, 0] }, { duration: 400 })
              : run(
                  c,
                  { opacity: [1, 0], transform: ['translate(-50%,0px) scale(1)', 'translate(-50%,-4px) scale(.96)'] },
                  { duration: 400 },
                ),
          )
        })()
      },
      hide() {
        const c = el.current
        seq.current++ // a pending hold or fade-out of the old pill does nothing now
        cur.current?.cancel()
        cur.current = undefined
        if (c) {
          c.style.removeProperty('opacity')
          c.style.removeProperty('transform')
        }
      },
    }),
    [],
  )
  return <div ref={el} className="chip" role="status" aria-live="polite" />
}
