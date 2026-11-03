import { useRef } from 'react'
import { animate } from 'motion'
import { Minus, RefreshCw, X } from 'lucide-react'
import { minimize, quit, send } from '@/lib/bridge'
import { springEase } from '@/lib/easing'
import type { Lang } from '@/lib/types'
import { Flags } from './Flags'

interface Props {
  lang: Lang
  onLang: (l: Lang) => void
  /** The refresh button only exists while the rings are shown. */
  showRefresh: boolean
}

// Prototype press: scale 1 → .86 (at 25 %) → 1, the whole effect eased by SPRING.
const pressScale = (p: number) => (p < 0.25 ? 1 - 0.14 * (p / 0.25) : 0.86 + (0.14 * (p - 0.25)) / 0.75)

/** Hover-only controls: flags (top-left), minimize/close (top-right), refresh (bottom-right). */
export function HoverControls({ lang, onLang, showRefresh }: Props) {
  const rf = useRef<HTMLButtonElement>(null)
  const icon = useRef<SVGSVGElement>(null)
  const turning = useRef(false)
  const fr = lang === 'fr'

  const onRefresh = () => {
    const btn = rf.current
    const ic = icon.current
    if (!btn || !ic || turning.current) return
    turning.current = true
    void animate(0, 1, {
      duration: 0.5,
      ease: springEase,
      onUpdate: (p) => {
        btn.style.transform = `scale(${pressScale(p)})`
      },
    }).then(() => {
      btn.style.transform = ''
    })
    void animate(ic, { rotate: [0, 360] }, { duration: 0.9, ease: springEase }).then(() => {
      ic.style.transform = ''
      turning.current = false
    })
    // Prototype timing: the request goes out while the icon is still turning.
    setTimeout(() => void send({ type: 'refresh' }), 350)
  }

  return (
    <>
      <div className="ctl">
        <button type="button" className="ib" title={fr ? 'Réduire' : 'Minimise'} onClick={() => void minimize()}>
          <Minus size={12} strokeWidth={2} />
        </button>
        <button type="button" className="ib close" title={fr ? 'Fermer' : 'Close'} onClick={() => void quit()}>
          <X size={12} strokeWidth={2} />
        </button>
      </div>
      <Flags lang={lang} onLang={onLang} />
      {showRefresh && (
        <button
          ref={rf}
          type="button"
          className="ib rf"
          title={fr ? 'Actualiser' : 'Refresh now'}
          onClick={onRefresh}
        >
          <RefreshCw ref={icon} size={12} strokeWidth={2} />
        </button>
      )}
    </>
  )
}
