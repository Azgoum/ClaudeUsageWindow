import { useImperativeHandle, useRef, type Ref } from 'react'
import type { AnimationPlaybackControls } from 'motion'
import type { PillKind } from '@/anim/pill'
import { springEase } from '@/anim/spring'
import { prefersReducedMotion, run } from '@/anim/stage'

export interface PillApi {
  show(kind: PillKind, text: string, holdMs: number): void
}

/**
 * The one reset pill (prototype `chip`). It sits inside the widget near the bottom: the window
 * is exactly the widget's box, and the longest French texts need two lines.
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
        cur.current?.stop()
        c.className = `chip ${kind}`
        c.textContent = text
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
    }),
    [],
  )
  return <div ref={el} className="chip" role="status" aria-live="polite" />
}
