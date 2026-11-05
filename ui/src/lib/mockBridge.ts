// Browser-only stand-in for the Tauri backend, so the UI can be driven by Playwright.
// Scenario via the URL: ?mock=connected|waiting|disconnected|timedout|needsFirefox|expired|error|near|idle|twoOrgs
import type { Bridge, UnlistenFn } from './bridge'
import type { AppState, CommandDto, Lang, OrgSummary, Prefs, ResetEvent, RingView } from './types'

/**
 * planned / surprise: the session ring; plannedWeekly / surpriseWeekly: the weekly ring;
 * both: two surprises; bothPlanned: two planned resets.
 */
type ResetKind = 'planned' | 'surprise' | 'both' | 'bothPlanned' | 'plannedWeekly' | 'surpriseWeekly'

export interface MockApi {
  push(state: Partial<AppState>): void
  reset(kind: ResetKind): void
  prefs: Prefs
  sent: CommandDto[]
}

declare global {
  interface Window {
    __mock?: MockApi
  }
}

const H = 3_600_000
const iso = (ms: number) => new Date(ms).toISOString()

// Named like real claude.ai orgs: a personal org is "<email>'s Organization".
const ORG_PERSONAL: OrgSummary = { uuid: 'org-personal', name: "jane@example.com's Organization", plan: 'max' }
const ORG_TEAM: OrgSummary = { uuid: 'org-team', name: 'Acme Studio', plan: 'team' }

/** Next Friday 23:59 local time (always in the future). */
function nextFriday2359(now: number): number {
  const d = new Date(now)
  d.setHours(23, 59, 0, 0)
  d.setDate(d.getDate() + ((5 - d.getDay() + 7) % 7))
  if (d.getTime() <= now) d.setDate(d.getDate() + 7)
  return d.getTime()
}

function rings(
  now: number,
  s: { used: number; expected: number | null; pace: RingView['pace']; resetsAt: number | null },
  w: { used: number; expected: number; pace: RingView['pace'] },
): [RingView, RingView] {
  return [
    { key: 'session', used: s.used, resetsAt: s.resetsAt === null ? null : iso(s.resetsAt), expected: s.expected, pace: s.pace },
    { key: 'weekly', used: w.used, resetsAt: iso(nextFriday2359(now)), expected: w.expected, pace: w.pace },
  ]
}

function scenario(kind: string, now: number): AppState {
  const base: AppState = {
    conn: { state: 'connected' },
    org: ORG_PERSONAL,
    orgs: [ORG_PERSONAL],
    rings: rings(
      now,
      { used: 62, expected: 48, pace: 'ahead', resetsAt: now + 2.5 * H },
      { used: 34, expected: 45, pace: 'room' },
    ),
    lastOk: iso(now),
    error: null,
    resets: [],
    resetSeq: 0,
  }
  const away = { ...base, org: null, orgs: [], rings: null, lastOk: null }
  switch (kind) {
    case 'waiting':
      return { ...away, conn: { state: 'waiting' } }
    case 'disconnected':
      return { ...away, conn: { state: 'disconnected', timedOut: false } }
    case 'timedout':
      return { ...away, conn: { state: 'disconnected', timedOut: true } }
    case 'needsFirefox':
      return { ...away, conn: { state: 'needsFirefox' } }
    case 'expired':
      return { ...base, conn: { state: 'expired' } }
    case 'error':
      return { ...base, lastOk: iso(now - 4 * 60_000), error: { retryAt: iso(now + 2 * 60_000) } }
    case 'near':
      return {
        ...base,
        rings: rings(
          now,
          { used: 94, expected: 80, pace: 'ahead', resetsAt: now + 1.2 * H },
          { used: 88, expected: 70, pace: 'ahead' },
        ),
      }
    case 'idle':
      return {
        ...base,
        rings: rings(now, { used: 0, expected: null, pace: 'idle', resetsAt: null }, { used: 34, expected: 45, pace: 'room' }),
      }
    case 'twoOrgs':
      return { ...base, orgs: [ORG_PERSONAL, ORG_TEAM] }
    default:
      return base
  }
}

const PREFS_KEY = 'mock.prefs'

function loadPrefs(): Prefs {
  const def: Prefs = {
    language: (navigator.language || 'en').toLowerCase().startsWith('fr') ? 'fr' : 'en',
    alwaysOnTop: true,
    startWithWindows: true,
  }
  try {
    const raw = localStorage.getItem(PREFS_KEY)
    return raw ? { ...def, ...(JSON.parse(raw) as Partial<Prefs>) } : def
  } catch {
    return def
  }
}

const listeners = new Set<(s: AppState) => void>()
let state = scenario(new URLSearchParams(location.search).get('mock') ?? 'connected', Date.now())
const prefs = loadPrefs()
const sent: CommandDto[] = []
let connectTimer: ReturnType<typeof setTimeout> | undefined

const emit = () => listeners.forEach((cb) => cb(structuredClone(state)))
const set = (next: AppState) => {
  state = next
  emit()
}
const savePrefs = () => {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs))
  } catch {
    /* storage unavailable: prefs stay in memory */
  }
}

/** How early the mock's surprises come: 2h14 (session), 3d 12h (weekly). */
const EARLY_SECS: Record<ResetEvent['key'], number> = { session: 8040, weekly: 302_400 }

function reset(kind: ResetKind) {
  if (!state.rings) return
  const now = Date.now()
  const keys: ResetEvent['key'][] =
    kind === 'both' || kind === 'bothPlanned'
      ? ['session', 'weekly']
      : kind === 'plannedWeekly' || kind === 'surpriseWeekly'
        ? ['weekly']
        : ['session']
  const planned = kind === 'planned' || kind === 'bothPlanned' || kind === 'plannedWeekly'
  const events: ResetEvent[] = keys.map((key) => {
    const fromUsed = state.rings!.find((r) => r.key === key)!.used
    return planned
      ? { key, kind: 'planned', fromUsed }
      : { key, kind: 'surprise', earlySecs: EARLY_SECS[key], fromUsed }
  })
  const next = state.rings.map((r) =>
    keys.includes(r.key)
      ? {
          ...r,
          used: 0,
          pace: 'room' as const,
          resetsAt: iso(r.key === 'session' ? now + 5 * H : nextFriday2359(now)),
          expected: r.key === 'session' ? 0 : r.expected,
        }
      : r,
  ) as [RingView, RingView]
  set({ ...state, rings: next, resets: events, resetSeq: state.resetSeq + 1, lastOk: iso(now) })
}

function simulate(cmd: CommandDto) {
  switch (cmd.type) {
    case 'refresh':
      set({ ...state, lastOk: iso(Date.now()), error: null })
      break
    case 'selectOrg': {
      const org = state.orgs.find((o) => o.uuid === cmd.uuid)
      if (org) set({ ...state, org })
      break
    }
    case 'connect':
      clearTimeout(connectTimer)
      set({ ...scenario('waiting', Date.now()) })
      connectTimer = setTimeout(() => set(scenario('connected', Date.now())), 1500)
      break
    case 'cancelConnect':
    case 'disconnect':
      clearTimeout(connectTimer)
      set(scenario('disconnected', Date.now()))
      break
  }
}

export const mockBridge: Bridge = {
  async onState(cb) {
    const own = (s: AppState) => cb(s) // unique entry even if the same callback subscribes twice
    listeners.add(own)
    const un: UnlistenFn = () => void listeners.delete(own)
    return un
  },
  getState: async () => structuredClone(state),
  async send(cmd) {
    sent.push(cmd)
    simulate(cmd)
  },
  getPrefs: async () => ({ ...prefs }),
  async setLanguage(lang: Lang) {
    prefs.language = lang
    savePrefs()
  },
  async setAlwaysOnTop(on) {
    prefs.alwaysOnTop = on
    savePrefs()
  },
  async setStartWithWindows(on) {
    prefs.startWithWindows = on
    savePrefs()
  },
  quit: async () => console.info('[mock] quit'),
  minimize: async () => console.info('[mock] minimize'),
  startDrag: async () => console.info('[mock] startDrag'),
  openUrl: async (url) => console.info('[mock] openUrl', url),
  fitWindow: async () => 0, // no window to resize in the browser
}

window.__mock = {
  push(partial) {
    set({ ...state, ...partial })
  },
  reset,
  prefs,
  sent,
}
