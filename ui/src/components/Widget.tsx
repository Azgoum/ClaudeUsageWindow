import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
} from 'react'
import { flushSync } from 'react-dom'
import { lockKeys, release, type Locks } from '@/anim/lock'
import { pillFor, type PillSpec } from '@/anim/pill'
import { playPlanned } from '@/anim/planned'
import { stopStage } from '@/anim/stage'
import { playSurprise, stopGlow } from '@/anim/surprise'
import { fitWindow, startDrag } from '@/lib/bridge'
import { errLine, isNearLimit, tipTitle } from '@/lib/ring'
import { maxRoom, menuRoom, tipRoom, withRingRoom, type RingRooms } from '@/lib/room'
import type { AppState, Lang, LimitKey, Prefs, ResetEvent } from '@/lib/types'
import { useNow } from '@/lib/useNow'
import { cn } from '@/lib/utils'
import { ConnectPanel } from './ConnectPanel'
import { HoverControls } from './HoverControls'
import { Pill, type PillApi } from './Pill'
import { Ring, type RingHandle } from './Ring'
import { RingTooltip } from './RingTooltip'
import { Sparks, type SparksApi } from './Sparks'
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

/**
 * A ring never stays locked longer than this, even if its animation stalls (window minimized,
 * frames paused): a bit more than the longest lock (surprise, about 3.05 s).
 */
const LOCK_SAFETY_MS = 4500

interface Batch {
  seq: number
  events: ResetEvent[]
  pill: PillSpec | null
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
  const sessionRef = ringRefs?.session ?? ownSession
  const weeklyRef = ringRefs?.weekly ?? ownWeekly

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

  // ---- reset animations (Task 14) ----
  // A new resetSeq locks the affected rings in the same render that brings the new values, so
  // they keep showing the old ones until their animation hands over (no flash of the new view).
  const [seenSeq, setSeenSeq] = useState(state.resetSeq)
  const [locks, setLocks] = useState<Locks>({})
  const [batch, setBatch] = useState<Batch | null>(null)
  if (state.resetSeq !== seenSeq) {
    setSeenSeq(state.resetSeq)
    if (connected && state.resets.length > 0) {
      setLocks((l) => lockKeys(l, state.resets.map((e) => e.key), state.resetSeq))
      setBatch({ seq: state.resetSeq, events: state.resets, pill: pillFor(state.resets, lang) })
    }
  }
  // Rings gone (disconnect, expired): nothing may stay locked for when they come back.
  if (!connected && Object.keys(locks).length > 0) setLocks({})
  // Mirror of `locks` for the async callbacks: which batch owns each ring right now.
  const owner = useRef<Locks>({})
  // Rings being animated, so a disconnect can stop them after React has cleared the ring refs.
  const animating = useRef(new Set<Element>())
  const pill = useRef<PillApi>(null)
  const sparks = useRef<SparksApi>(null)

  useLayoutEffect(() => {
    const el = rootRef.current
    if (!batch || !el) return
    const { seq, events, pill: spec } = batch
    owner.current = lockKeys(owner.current, events.map((e) => e.key), seq)
    let shown = false
    const showPill = () => {
      // Only while this batch still owns a ring: never after a disconnect or a newer batch.
      if (shown || !spec || !Object.values(owner.current).includes(seq)) return
      shown = true
      pill.current?.show(spec.kind, spec.text, spec.holdMs)
    }
    for (const ev of events) {
      const h = (ev.key === 'session' ? sessionRef : weeklyRef).current
      let safety: ReturnType<typeof setTimeout> | undefined
      const done = () => {
        clearTimeout(safety)
        if (owner.current[ev.key] !== seq) return // a newer batch owns this ring, or the rings are gone
        // An animation that stalled (window minimized) never reached its pill moment: the batch
        // still gets its pill (before the release, while the batch still owns the ring).
        showPill()
        owner.current = release(owner.current, ev.key, seq)
        if (svg) animating.current.delete(svg)
        stopStage(svg)
        h?.paint()
        flushSync(() => setLocks((l) => release(l, ev.key, seq)))
      }
      if (!h) {
        // No ring to animate: just unlock, after the effect (flushSync can't run inside one).
        owner.current = release(owner.current, ev.key, seq)
        queueMicrotask(() => setLocks((l) => release(l, ev.key, seq)))
        continue
      }
      const svg = h.svg
      if (svg) animating.current.add(svg)
      safety = setTimeout(done, LOCK_SAFETY_MS)
      const p =
        ev.kind === 'planned'
          ? playPlanned(h, ev.fromUsed, showPill)
          : playSurprise(h, el, ev.fromUsed, sparks.current, showPill)
      void p.then(done, done)
    }
    // No cleanup: each ring's own promise or safety timer releases it, and a newer batch takes
    // over through `owner`. The ring refs are stable: only a new batch starts animations.
  }, [batch, sessionRef, weeklyRef])

  useEffect(() => {
    if (connected) return
    owner.current = {}
    for (const svg of animating.current) stopStage(svg)
    animating.current.clear()
    // No glow, sparks or pill left over the connect panel.
    stopGlow()
    sparks.current?.clear()
    pill.current?.hide()
  }, [connected, sessionRef, weeklyRef])
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
      <div className="glow" aria-hidden />
      <Sparks ref={sparks} />
      <Pill ref={pill} />
      {connected ? (
        <>
          <div className="rings">
            {state.rings!.map((r) => (
              <RingTooltip
                key={r.key}
                view={r}
                title={tipTitle(state.org, state.orgs, lang)}
                lastOk={state.lastOk}
                lang={lang}
                disabled={menuOpen || dragging}
                onPopupHeight={onTipHeight}
              >
                <Ring
                  view={r}
                  lang={lang}
                  now={now}
                  held={locks[r.key] !== undefined}
                  index={r.key === 'session' ? 0 : 1}
                  ref={r.key === 'session' ? sessionRef : weeklyRef}
                />
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
