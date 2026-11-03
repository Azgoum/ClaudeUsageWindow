// Mirrors the Rust backend contract (crates/limits-core engine.rs, detect.rs, pace.rs; src-tauri lib.rs).

export type Lang = 'fr' | 'en'
export type LimitKey = 'session' | 'weekly'
export type Pace = 'ahead' | 'ontrack' | 'room' | 'idle'

export type Conn =
  | { state: 'disconnected'; timedOut: boolean }
  | { state: 'needsFirefox' }
  | { state: 'waiting' }
  | { state: 'connected' }
  | { state: 'expired' }

export interface RingView {
  key: LimitKey
  /** 0-100 */
  used: number
  /** ISO 8601 UTC, null when no session is running */
  resetsAt: string | null
  /** 0-100 expected usage at this point of the window, null when unknown */
  expected: number | null
  pace: Pace
}

export interface OrgSummary {
  uuid: string
  name: string
}

export type ResetEvent = { key: LimitKey; fromUsed: number } & (
  | { kind: 'planned' }
  | { kind: 'surprise'; earlySecs: number }
)

export interface AppState {
  conn: Conn
  org: OrgSummary | null
  orgs: OrgSummary[]
  /** [session, weekly] */
  rings: [RingView, RingView] | null
  lastOk: string | null
  error: { retryAt: string } | null
  /** Events of the latest detection; animate when resetSeq changes. */
  resets: ResetEvent[]
  resetSeq: number
}

export type CommandDto =
  | { type: 'connect' | 'cancelConnect' | 'refresh' | 'disconnect' }
  | { type: 'selectOrg'; uuid: string }

export interface Prefs {
  language: Lang
  alwaysOnTop: boolean
  startWithWindows: boolean
}
