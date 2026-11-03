import type { ReactElement, ReactNode } from 'react'
import { ContextMenu } from '@base-ui/react/context-menu'
import { quit, send, setAlwaysOnTop, setStartWithWindows } from '@/lib/bridge'
import { t } from '@/lib/i18n'
import type { AppState, Lang, Prefs } from '@/lib/types'
import { cn } from '@/lib/utils'

interface Props {
  state: AppState
  prefs: Prefs
  lang: Lang
  onPrefs: (p: Partial<Prefs>) => void
  onOpenChange?: (open: boolean) => void
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
export function WidgetMenu({ state, prefs, lang, onPrefs, onOpenChange, render, children }: Props) {
  const L = t(lang)
  const connected = state.conn.state === 'connected'
  return (
    <ContextMenu.Root onOpenChange={(open) => onOpenChange?.(open)}>
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
          <ContextMenu.Popup className="menu" data-nodrag>
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
