import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { AppState, CommandDto, Lang, Prefs } from './types'

export type { UnlistenFn }

/** Everything the UI needs from the backend. Implemented by Tauri and by the browser mock. */
export interface Bridge {
  onState(cb: (s: AppState) => void): Promise<UnlistenFn>
  getState(): Promise<AppState>
  send(cmd: CommandDto): Promise<void>
  getPrefs(): Promise<Prefs>
  setLanguage(lang: Lang): Promise<void>
  setAlwaysOnTop(on: boolean): Promise<void>
  setStartWithWindows(on: boolean): Promise<void>
  quit(): Promise<void>
  minimize(): Promise<void>
  startDrag(): Promise<void>
}

const tauriBridge: Bridge = {
  onState: (cb) => listen<AppState>('state', (e) => cb(e.payload)),
  getState: () => invoke<AppState>('get_state'),
  send: (cmd) => invoke('send_command', { cmd }),
  getPrefs: () => invoke<Prefs>('get_prefs'),
  setLanguage: (lang) => invoke('set_language', { lang }),
  setAlwaysOnTop: (on) => invoke('set_always_on_top', { on }),
  setStartWithWindows: (on) => invoke('set_start_with_windows', { on }),
  quit: () => invoke('quit'),
  minimize: () => getCurrentWindow().minimize(),
  startDrag: () => getCurrentWindow().startDragging(),
}

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// Outside Tauri (plain browser, Playwright) fall back to the scripted mock. The dynamic
// import keeps it out of the Tauri bundle's main chunk and never loads it there.
let mockPromise: Promise<Bridge> | undefined
const loadMock = () => (mockPromise ??= import('./mockBridge').then((m) => m.mockBridge))
const lazy = <K extends keyof Bridge>(k: K) =>
  ((...args: unknown[]) =>
    loadMock().then((b) => (b[k] as (...a: unknown[]) => unknown)(...args))) as Bridge[K]

const impl: Bridge = inTauri
  ? tauriBridge
  : {
      onState: lazy('onState'),
      getState: lazy('getState'),
      send: lazy('send'),
      getPrefs: lazy('getPrefs'),
      setLanguage: lazy('setLanguage'),
      setAlwaysOnTop: lazy('setAlwaysOnTop'),
      setStartWithWindows: lazy('setStartWithWindows'),
      quit: lazy('quit'),
      minimize: lazy('minimize'),
      startDrag: lazy('startDrag'),
    }

export const onState = impl.onState
export const getState = impl.getState
export const send = impl.send
export const getPrefs = impl.getPrefs
export const setLanguage = impl.setLanguage
export const setAlwaysOnTop = impl.setAlwaysOnTop
export const setStartWithWindows = impl.setStartWithWindows
export const quit = impl.quit
export const minimize = impl.minimize
export const startDrag = impl.startDrag
