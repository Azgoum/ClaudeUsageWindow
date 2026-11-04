import { useCallback, useState, type ReactElement } from 'react'
import { Tooltip } from '@base-ui/react/tooltip'
import { agoText, tipModel } from '@/lib/ring'
import type { Lang, LimitKey, RingView } from '@/lib/types'
import { useNow } from '@/lib/useNow'

interface Props {
  view: RingView
  /** Plan or org name (see `tipTitle`). */
  title: string
  lastOk: string | null
  lang: Lang
  disabled?: boolean
  /** Popup height while it is mounted, then 0, so the window can make room for it. */
  onPopupHeight?: (key: LimitKey, h: number) => void
  children: ReactElement
}

/** Hover card under a ring (prototype `.tip`); its text refreshes every second while open. */
export function RingTooltip({ view, title, lastOk, lang, disabled = false, onPopupHeight, children }: Props) {
  const [open, setOpen] = useState(false)
  // Stable, so the per-second re-render doesn't report 0 then h again (shrink/grow flicker).
  const popupRef = useCallback(
    (el: HTMLDivElement | null) => {
      if (!el) return
      onPopupHeight?.(view.key, el.getBoundingClientRect().height)
      return () => onPopupHeight?.(view.key, 0)
    },
    [onPopupHeight, view.key],
  )
  const now = useNow(1000, open)
  const m = tipModel(view, now, lang)
  const ago = agoText(lastOk, now, lang)
  return (
    <Tooltip.Root open={open && !disabled} onOpenChange={setOpen} disabled={disabled}>
      <Tooltip.Trigger delay={0} closeDelay={0} render={<div className="rg-hit" />}>
        {children}
      </Tooltip.Trigger>
      <Tooltip.Portal>
        {/* Below the ring; flips above (10 px gap, as the prototype) when it doesn't fit. */}
        <Tooltip.Positioner
          side="bottom"
          sideOffset={({ side }) => (side === 'top' ? 10 : 16)}
          collisionPadding={6}
          className="tip-pos"
        >
          <Tooltip.Popup ref={popupRef} className="tip">
            <div className="tt">
              {title} {ago && <span>· {ago}</span>}
            </div>
            {m.kind === 'idle' ? (
              <>
                <b>{m.title}</b> · {m.line}
                <br />
                {m.sub}
              </>
            ) : (
              <>
                <b>{m.title}</b> · {m.usage}
                {m.pace && (
                  <>
                    <br />
                    <b style={{ color: m.pace.color }}>{m.pace.text}</b>
                  </>
                )}
                {m.resets && (
                  <>
                    <br />
                    {m.resets}
                  </>
                )}
              </>
            )}
          </Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  )
}
