import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react'
import { PACE_COLOR } from '@/lib/colors'
import { resetLine } from '@/lib/format'
import { t } from '@/lib/i18n'
import { CIRC, arcOffset, numFill, ringNumber, tickAngle, tickOpacity } from '@/lib/ring'
import type { Lang, RingView } from '@/lib/types'

/** Live DOM nodes of one ring, for the reset animations (Task 14). */
export interface RingHandle {
  readonly svg: SVGSVGElement | null
  readonly arc: SVGCircleElement | null
  readonly glint: SVGCircleElement | null
  readonly tick: SVGLineElement | null
  readonly num: SVGTextElement | null
  readonly ok: HTMLSpanElement | null
}

interface Props {
  view: RingView
  lang: Lang
  now: number
  ref?: Ref<RingHandle>
}

export function Ring({ view, lang, now, ref }: Props) {
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
    }),
    [],
  )

  // Entrance: start empty, then let the CSS transition fill the ring (prototype paintRings in rAF).
  const [entered, setEntered] = useState(false)
  useEffect(() => {
    const id = requestAnimationFrame(() => setEntered(true))
    return () => cancelAnimationFrame(id)
  }, [])

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
          stroke={PACE_COLOR[view.pace]}
          strokeWidth="6"
          strokeLinecap="round"
          transform="rotate(-90 32 32)"
          strokeDasharray={CIRC}
          strokeDashoffset={entered ? arcOffset(view.used) : CIRC}
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
          y="36.5"
          textAnchor="middle"
          fontSize="13.5"
          fontWeight="750"
          fontFamily="Inter, Segoe UI, system-ui"
          fill={numFill(view)}
        >
          {ringNumber(view.used)}
          <tspan fontSize="9" dx="1">
            %
          </tspan>
        </text>
      </svg>
      <div className="name">
        {view.key === 'session' ? L.s : L.w}
        <span ref={ok} className="ok">
          ✓
        </span>
      </div>
      <div className="when">{resetLine(view.key, resetsAt, new Date(now), lang)}</div>
    </div>
  )
}
