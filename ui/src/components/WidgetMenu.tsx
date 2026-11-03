import { useCallback, useEffect, useState, type ReactElement, type ReactNode } from 'react'
import { ContextMenu } from '@base-ui/react/context-menu'
import { quit, send, setAlwaysOnTop, setStartWithWindows } from '@/lib/bridge'
import { t } from '@/lib/i18n'
import { menuShows } from '@/lib/menu'
import type { AppState, Lang, Prefs } from '@/lib/types'
import { cn } from '@/lib/utils'

interface Props {
  state: AppState
  prefs: Prefs
  lang: Lang
  onPrefs: (p: Partial<Prefs>) => void
  onOpenChange?: (open: boolean) => void
  /** Popup height while it is mounted, then 0, so the window can make room for it. */
  onPopupHeight?: (h: number) => void
  /** The widget root; right-clicking anywhere on it opens the menu. */
  render: ReactElement
  children: ReactNode
}

function Item({ chk, danger, onClick, children }: { chk?: boolean; danger?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <ContextMenu.Item className={cn('it', chk && 'chk', danger && 'danger')} onClick={onClick}>
      {children}
    </ContextMenu.Item>
  )
}

const Sep = () => <ContextMenu.Separator className="sep" />

/** Right-click menu, styled like the prototype's `.menu`. */
export function WidgetMenu({ state, prefs, lang, onPrefs, onOpenChange, onPopupHeight, render, children }: Props) {
  const L = t(lang)
  const [open, setOpen] = useState(false)
  const change = (o: boolean) => {
    setOpen(o)
    onOpenChange?.(o)
  }
  // Like a native menu, close when the user clicks into another app (the window loses focus).
  useEffect(() => {
    if (!open) return
    const close = () => {
      setOpen(false)
      onOpenChange?.(false)
    }
    window.addEventListener('blur', close)
    return () => window.removeEventListener('blur', close)
  }, [open, onOpenChange])
  const popupRef = useCallback(
    (el: HTMLDivElement | null) => {
      if (!el) return
      onPopupHeight?.(el.getBoundingClientRect().height)
      return () => onPopupHeight?.(0)
    },
    [onPopupHeight],
  )
  const connected = menuShows(state.conn).connectedItems
  return (
    <ContextMenu.Root open={open} onOpenChange={change}>
      <ContextMenu.Trigger render={render}>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        {/* Top-left corner at the cursor, clamped inside the viewport (prototype), never flipped. */}
        <ContextMenu.Positioner
          side="bottom"
          align="start"
          sideOffset={0}
          alignOffset={0}
          collisionAvoidance={{ side: 'shift', align: 'shift' }}
          collisionPadding={6}
          className="menu-pos"
        >
          <ContextMenu.Popup ref={popupRef} className="menu" data-nodrag>
            {connected && (
              <>
                <Item onClick={() => void send({ type: 'refresh' })}>{L.mRefresh}</Item>
                <Sep />
                {state.orgs.length > 1 && (
                  <>
                    <ContextMenu.Group>
                      <ContextMenu.GroupLabel className="hd">{L.mOrg}</ContextMenu.GroupLabel>
                      {state.orgs.map((o) => (
                        <Item
                          key={o.uuid}
                          chk={o.uuid === state.org?.uuid}
                          onClick={() => void send({ type: 'selectOrg', uuid: o.uuid })}
                        >
                          {o.name}
                        </Item>
                      ))}
                    </ContextMenu.Group>
                    <Sep />
                  </>
                )}
              </>
            )}
            <Item
              chk={prefs.alwaysOnTop}
              onClick={() => {
                onPrefs({ alwaysOnTop: !prefs.alwaysOnTop })
                void setAlwaysOnTop(!prefs.alwaysOnTop)
              }}
            >
              {L.mTop}
            </Item>
            <Item
              chk={prefs.startWithWindows}
              onClick={() => {
                onPrefs({ startWithWindows: !prefs.startWithWindows })
                void setStartWithWindows(!prefs.startWithWindows)
              }}
            >
              {L.mBoot}
            </Item>
            <Sep />
            {connected && <Item onClick={() => void send({ type: 'disconnect' })}>{L.mDisc}</Item>}
            <Item danger onClick={() => void quit()}>
              {L.mQuit}
            </Item>
          </ContextMenu.Popup>
        </ContextMenu.Positioner>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  )
}
