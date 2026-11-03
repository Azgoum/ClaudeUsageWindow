import { useRef, useState, type PointerEvent, type RefObject } from 'react'
import { startDrag } from '@/lib/bridge'
import { errLine, isNearLimit } from '@/lib/ring'
import type { AppState, Lang, Prefs } from '@/lib/types'
import { useNow } from '@/lib/useNow'
import { cn } from '@/lib/utils'
import { HoverControls } from './HoverControls'
import { Ring, type RingHandle } from './Ring'
import { RingTooltip } from './RingTooltip'
import { WidgetMenu } from './WidgetMenu'

interface Props {
  state: AppState
  prefs: Prefs
  lang: Lang
  onLang: (l: Lang) => void
  onPrefs: (p: Partial<Prefs>) => void
  /** Ring DOM handles [session, weekly], for the reset animations (Task 14). */
  ringRefs?: { session: RefObject<RingHandle | null>; weekly: RefObject<RingHandle | null> }
}

// Drag the window from anywhere except the controls (and never on right-click).
function onPointerDown(e: PointerEvent<HTMLDivElement>) {
  if (e.button !== 0) return
  if ((e.target as Element).closest('button, [data-nodrag]')) return
  void startDrag()
}

export function Widget({ state, prefs, lang, onLang, onPrefs, ringRefs }: Props) {
  const [menuOpen, setMenuOpen] = useState(false)
  const ownSession = useRef<RingHandle>(null)
  const ownWeekly = useRef<RingHandle>(null)
  const refs = ringRefs ?? { session: ownSession, weekly: ownWeekly }

  const connected = state.conn.state === 'connected' && state.rings !== null
  const err = connected && state.error !== null
  const warn = connected && !err && isNearLimit(state.rings)
  // Seconds matter only for the retry countdown; otherwise a slow clock keeps reset lines fresh.
  const now = useNow(err ? 1000 : 30_000)

  const root = <div className={cn('wd', warn && 'warn', err && 'err')} onPointerDown={onPointerDown} />

  return (
    <WidgetMenu state={state} prefs={prefs} lang={lang} onPrefs={onPrefs} onOpenChange={setMenuOpen} render={root}>
      <HoverControls lang={lang} onLang={onLang} showRefresh={connected} />
      {connected ? (
        <>
          <div className="rings">
            {state.rings!.map((r) => (
              <RingTooltip
                key={r.key}
                view={r}
                orgName={state.org?.name ?? ''}
                lastOk={state.lastOk}
                lang={lang}
                disabled={menuOpen}
              >
                <Ring view={r} lang={lang} now={now} ref={r.key === 'session' ? refs.session : refs.weekly} />
              </RingTooltip>
            ))}
          </div>
          <div className="foot">
            <span className="msg">{err ? errLine(state.error!.retryAt, now, lang) : ''}</span>
          </div>
        </>
      ) : (
        // Placeholder until Task 13's ConnectPanel lands.
        <div className="st" data-conn={state.conn.state}>
          {state.conn.state}
        </div>
      )}
    </WidgetMenu>
  )
}
