import { useCallback, useEffect, useRef, useState, type MouseEvent, type PointerEvent, type RefObject } from 'react'
import { fitWindow, startDrag } from '@/lib/bridge'
import { errLine, isNearLimit } from '@/lib/ring'
import { maxRoom, menuRoom, tipRoom, withRingRoom, type RingRooms } from '@/lib/room'
import type { AppState, Lang, LimitKey, Prefs } from '@/lib/types'
import { useNow } from '@/lib/useNow'
import { cn } from '@/lib/utils'
import { ConnectPanel } from './ConnectPanel'
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

interface Size {
  w: number
  h: number
}

/**
 * Keeps the window exactly the widget's size, plus `extra` px while a tooltip or menu is open.
 * Returns how far down the widget must sit when the window had to grow upward.
 */
function useWindowFit(extra: number) {
  const [size, setSize] = useState<Size | null>(null)
  const [offset, setOffset] = useState(0)
  const seq = useRef(0)

  // Callback ref: observes whichever element is current (it can be re-created), never a detached one.
  const measure = useCallback(
    (el: HTMLDivElement | null) => {
      if (!el) return
      const ro = new ResizeObserver(([e]) => {
        const b = e.borderBoxSize[0]
        if (b.inlineSize < 1 || b.blockSize < 1) return // detached or not laid out yet
        setSize((s) => (s && s.w === b.inlineSize && s.h === b.blockSize ? s : { w: b.inlineSize, h: b.blockSize }))
      })
      ro.observe(el)
      return () => ro.disconnect()
    },
    [],
  )

  useEffect(() => {
    if (!size) return
    const n = ++seq.current
    void fitWindow(size.w, size.h, extra).then((o) => {
      if (n === seq.current) setOffset(o)
    })
  }, [size, extra])

  /** Back to the resting size and position right away (before a drag). */
  const settle = useCallback(async () => {
    if (!size) return
    const n = ++seq.current
    const o = await fitWindow(size.w, size.h, 0)
    if (n === seq.current) setOffset(o)
  }, [size])

  return { offset, settle, measure }
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

  // Room for the open popups, in CSS px below the widget.
  const rootRef = useRef<HTMLDivElement>(null)
  const cursorY = useRef(0)
  // Per ring: moving from one tooltip to the other, the old one clears only its own entry.
  const [tipRooms, setTipRooms] = useState<RingRooms>({})
  const tipExtra = maxRoom(tipRooms)
  const [menuExtra, setMenuExtra] = useState(0)
  const extra = Math.max(tipExtra, menuExtra)
  const { offset, settle, measure } = useWindowFit(extra)
  const setRoot = useCallback(
    (el: HTMLDivElement | null) => {
      rootRef.current = el
      return measure(el)
    },
    [measure],
  )

  const onTipHeight = useCallback((key: LimitKey, h: number) => {
    const el = rootRef.current
    const trigger = el?.querySelector('.rg-hit')
    const room =
      el && trigger && h > 0
        ? tipRoom(trigger.getBoundingClientRect().bottom - el.getBoundingClientRect().top, h, el.offsetHeight)
        : 0
    setTipRooms((rooms) => withRingRoom(rooms, key, room))
  }, [])
  const onMenuHeight = useCallback((h: number) => {
    const el = rootRef.current
    setMenuExtra(el && h ? menuRoom(cursorY.current, h, el.getBoundingClientRect().height) : 0)
  }, [])
  const onContextMenu = (e: MouseEvent<HTMLDivElement>) => {
    cursorY.current = e.clientY - e.currentTarget.getBoundingClientRect().top
  }

  // Drag the window from the widget only, never from the controls or on right-click.
  // A grown window first goes back to rest, so the drag moves the resting window.
  // No tooltip while dragging: growing the window mid-drag would fight the OS move loop.
  const [dragging, setDragging] = useState(false)
  useEffect(() => {
    if (!dragging) return
    // The OS move loop swallows pointerup: the first buttonless move anywhere ends the drag.
    const end = (e: globalThis.PointerEvent) => e.buttons === 0 && setDragging(false)
    document.addEventListener('pointermove', end)
    document.addEventListener('pointerup', end)
    return () => {
      document.removeEventListener('pointermove', end)
      document.removeEventListener('pointerup', end)
    }
  }, [dragging])
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    if ((e.target as Element).closest('button, [data-nodrag]')) return
    setDragging(true)
    if (extra > 0) {
      setTipRooms({})
      setMenuExtra(0)
      void settle().then(() => startDrag())
    } else {
      void startDrag()
    }
  }

  const root = (
    <div
      ref={setRoot}
      className={cn('wd', warn && 'warn', err && 'err')}
      style={offset ? { top: offset } : undefined}
      onPointerDown={onPointerDown}
      onContextMenu={onContextMenu}
    />
  )

  return (
    <WidgetMenu
      state={state}
      prefs={prefs}
      lang={lang}
      onPrefs={onPrefs}
      onOpenChange={setMenuOpen}
      onPopupHeight={onMenuHeight}
      render={root}
    >
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
                disabled={menuOpen || dragging}
                onPopupHeight={onTipHeight}
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
        <ConnectPanel conn={state.conn} lang={lang} />
      )}
    </WidgetMenu>
  )
}
