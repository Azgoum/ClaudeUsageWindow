import { useImperativeHandle, useRef, type Ref } from 'react'
import { run } from '@/anim/stage'

export interface SparksApi {
  /**
   * Throws `n` sparks off a ring: they start on the circle (`radius` around `center`, in the
   * widget's padding-box px) within the first `arcFraction` of a turn from 12 o'clock.
   */
  burst(center: { x: number; y: number }, radius: number, arcFraction: number, n?: number): void
  /** Removes every spark still in flight. */
  clear(): void
}

const COLORS = ['#facc15', '#fde68a', '#fb923c', '#fff']

/** Gold sparks layer, clipped to the glass (the window ends at the widget's border). */
export function Sparks({ ref }: { ref?: Ref<SparksApi> }) {
  const layer = useRef<HTMLDivElement>(null)
  useImperativeHandle(
    ref,
    () => ({
      clear() {
        layer.current?.replaceChildren()
      },
      burst(center, radius, arcFraction, n = 30) {
        const el = layer.current
        if (!el) return
        // Port of the prototype's `sparks`.
        for (let i = 0; i < n; i++) {
          const a0 = -Math.PI / 2 + Math.random() * 2 * Math.PI * arcFraction
          const s = document.createElement('span')
          s.className = 'spark'
          s.style.background = COLORS[i % 4]
          s.style.left = `${center.x + Math.cos(a0) * radius - 2.5}px`
          s.style.top = `${center.y + Math.sin(a0) * radius - 2.5}px`
          el.appendChild(s)
          const d = 24 + Math.random() * 50
          void run(
            s,
            {
              transform: ['translate(0px,0px) scale(1)', `translate(${Math.cos(a0) * d}px,${Math.sin(a0) * d}px) scale(.2)`],
              opacity: [1, 0],
            },
            { duration: 700 + Math.random() * 600, ease: [0.1, 0.7, 0.3, 1] },
          ).finished.then(() => s.remove(), () => s.remove())
        }
      },
    }),
    [],
  )
  return <div ref={layer} className="sparks" aria-hidden />
}
