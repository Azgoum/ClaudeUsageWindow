// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { AppState, Prefs } from '@/lib/types'

vi.mock('@/lib/bridge', () => ({
  quit: vi.fn(() => Promise.resolve()),
  send: vi.fn(() => Promise.resolve()),
  setAlwaysOnTop: vi.fn(() => Promise.resolve()),
  setStartWithWindows: vi.fn(() => Promise.resolve()),
}))

const { quit } = await import('@/lib/bridge')
const { WidgetMenu } = await import('../WidgetMenu')

const state: AppState = {
  conn: { state: 'connected' },
  org: null,
  orgs: [],
  rings: null,
  lastOk: null,
  error: null,
  resets: [],
  resetSeq: 0,
}
const prefs: Prefs = { language: 'en', alwaysOnTop: true, startWithWindows: true }

afterEach(cleanup)

describe('WidgetMenu', () => {
  it('Quit calls the bridge exactly once', async () => {
    render(
      <WidgetMenu state={state} prefs={prefs} lang="en" onPrefs={() => {}} render={<div data-testid="root" />}>
        <span>widget</span>
      </WidgetMenu>,
    )
    await act(async () => {
      fireEvent.contextMenu(screen.getByTestId('root'), { clientX: 10, clientY: 10 })
    })
    const item = await screen.findByText('Quit')
    await act(async () => {
      fireEvent.click(item)
    })
    expect(quit).toHaveBeenCalledTimes(1)
  })
})
