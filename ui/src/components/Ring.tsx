import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from 'react'
import { prefersReducedMotion, run } from '@/anim/stage'
import { resetLine } from '@/lib/format'
import { t } from '@/lib/i18n'
import { CIRC, arcOffset, arcStroke, numFill, ringNumber, tickAngle, tickOpacity } from '@/lib/ring'
import type { Lang, RingView } from '@/lib/types'

/** Live DOM nodes of one ring, for the reset animations (Task 14). */
export interface RingHandle {
  readonly svg: SVGSVGElement | null
  readonly arc: SVGCircleElement | null
  readonly glint: SVGCircleElement | null
  readonly tick: SVGLineElement | null
  readonly num: SVGTextElement | null
  readonly ok: HTMLSpanElement | null
  /** Writes the latest `view` into the DOM (after an animation wrote its own values). */
  paint(): void
}

interface Props {
  view: RingView
  lang: Lang
  now: number
  /** While a reset animation plays, the ring keeps showing the view it had before (Task 14). */
  held?: boolean
  /** Position in the row, for the entrance stagger. */
  index?: number
  ref?: Ref<RingHandle>
}

/** Entrance (brainstorm mockup): arcs spring from empty, 1200 ms, 90 ms apart. */
const ENTER_MS = 1200
const ENTER_STAGGER_MS = 90
const ENTER_EASE = [0.2, 0.9, 0.25, 1.05] as const

export function Ring({ view: live, lang, now, held = false, index = 0, ref }: Props) {
  // Held: render the last view seen before the lock; the live one is applied on release.
  const [free, setFree] = useState(live)
  if (!held && free !== live) setFree(live)
  const view = held ? free : live
  const latest = useRef(live)
  useLayoutEffect(() => {
    latest.current = live
  })

  const svg = useRef<SVGSVGElement>(null)
  const arc = useRef<SVGCircleElement>(null)
  const glint = useRef<SVGCircleElement>(null)
  const tick = useRef<SVGLineElement>(null)
  const num = useRef<SVGTextElement>(null)
  const ok = useRef<HTMLSpanElement>(null)
  useImperativeHandle(
    ref,
    () => ({
      get svg() {
        return svg.current
      },
      get arc() {
        return arc.current
      },
      get glint() {
        return glint.current
      },
      get tick() {
        return tick.current
      },
      get num() {
        return num.current
      },
      get ok() {
        return ok.current
      },
      // Keep in sync with the JSX below: both map a view to the same attributes.
      paint() {
        // Same values React renders (prototype `paintRings`); the arc eases from where the
        // animation left it and the colour fades through the CSS transitions.
        const v = latest.current
        arc.current?.setAttribute('stroke-dashoffset', String(arcOffset(v.used)))
        arc.current?.setAttribute('stroke', arcStroke(v))
        num.current?.setAttribute('fill', numFill(v))
        const text = num.current?.firstChild
        if (text && !v.noLimit) text.nodeValue = String(ringNumber(v.used))
        if (tick.current) {
          tick.current.style.opacity = String(tickOpacity(v.expected))
          tick.current.style.transform = `rotate(${tickAngle(v.expected)}deg)`
        }
      },
    }),
    [],
  )

  // Entrance: the ring mounts when the rings first arrive (also after a reconnect) and its arc
  // springs from empty. The tick turns in through its CSS transition (prototype paintRings in rAF).
  const [entered, setEntered] = useState(false)
  useEffect(() => {
    const id = requestAnimationFrame(() => setEntered(true))
    return () => cancelAnimationFrame(id)
  }, [])
  useLayoutEffect(() => {
    const el = arc.current
    if (!el || prefersReducedMotion()) return
    el.style.transition = 'none'
    el.setAttribute('stroke-dashoffset', String(CIRC))
    const a = run(
      el,
      { strokeDashoffset: [CIRC, arcOffset(latest.current.used)] },
      { duration: ENTER_MS, delay: index * ENTER_STAGGER_MS, ease: [...ENTER_EASE] },
    )
    const entrance = { active: true }
    const done = () => {
      if (!entrance.active) return
      // motion leaves its last value as an inline style, which would beat the attribute.
      el.style.removeProperty('stroke-dashoffset')
      el.style.transition = ''
      // Values that changed meanwhile: React wrote them, the animation wrote over them.
      el.setAttribute('stroke-dashoffset', String(arcOffset(latest.current.used)))
    }
    void a.finished.then(done, done)
    return () => {
      entrance.active = false
      a.cancel()
      el.style.removeProperty('stroke-dashoffset')
      el.style.transition = ''
    }
    // Mount only (index never changes): the entrance plays once per appearance of the rings.
  }, [index])

  const L = t(lang)
  const resetsAt = view.resetsAt ? new Date(view.resetsAt) : null
  return (
    <div className="rg" data-k={view.key}>
      <svg ref={svg} width="64" height="64" viewBox="0 0 64 64">
        <circle cx="32" cy="32" r="23" fill="none" stroke="rgba(255,255,255,.08)" strokeWidth="6" />
        <circle
          ref={arc}
          className="arc"
          cx="32"
          cy="32"
          r="23"
          fill="none"
          stroke={arcStroke(view)}
          strokeWidth="6"
          strokeLinecap="round"
          transform="rotate(-90 32 32)"
          strokeDasharray={CIRC}
          strokeDashoffset={arcOffset(view.used)}
        />
        <circle
          ref={glint}
          className="glint"
          cx="32"
          cy="32"
          r="23"
          fill="none"
          stroke="#fff"
          strokeWidth="6"
          strokeLinecap="round"
          transform="rotate(-90 32 32)"
          strokeDasharray={`9 ${CIRC}`}
          strokeDashoffset="9"
          opacity="0"
          style={{ filter: 'blur(1.5px)' }}
        />
        <line
          ref={tick}
          className="tick"
          x1="32"
          y1="3"
          x2="32"
          y2="12"
          stroke="#fff"
          strokeWidth="2"
          strokeLinecap="round"
          style={{
            opacity: tickOpacity(view.expected),
            transform: `rotate(${entered ? tickAngle(view.expected) : 0}deg)`,
          }}
        />
        <text
          ref={num}
          className="num"
          x="32"
          y={view.noLimit ? 39 : 36.5}
          textAnchor="middle"
          fontSize={view.noLimit ? 24 : 13.5}
          fontWeight="750"
          fontFamily="Inter, Segoe UI, system-ui"
          fill={numFill(view)}
        >
          {view.noLimit ? (
            '∞'
          ) : (
            <>
              {ringNumber(view.used)}
              <tspan fontSize="9" dx="1">
                %
              </tspan>
            </>
          )}
        </text>
      </svg>
      <div className="name">
        {view.noLimit ? L.noLimit : view.key === 'session' ? L.s : L.w}
        <span ref={ok} className="ok">
          ✓
        </span>
      </div>
      <div className="when">{view.noLimit ? ' ' : resetLine(view.key, resetsAt, new Date(now), lang)}</div>
    </div>
  )
}
