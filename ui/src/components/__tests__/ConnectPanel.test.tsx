import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { ConnectPanel } from '../ConnectPanel'

const now = Date.parse('2026-11-03T12:00:00Z')
const connected = { state: 'connected' } as const

describe('ConnectPanel while connected without rings', () => {
  it('shows the retry line when the first poll failed', () => {
    const html = renderToStaticMarkup(
      <ConnectPanel conn={connected} lang="en" error={{ retryAt: '2026-11-03T12:01:00Z' }} now={now} />,
    )
    expect(html).toContain('⚠ claude.ai not responding · retry in')
    expect(html).not.toContain('Connect to claude.ai')
  })
  it('stays empty while the first poll is in flight', () => {
    expect(renderToStaticMarkup(<ConnectPanel conn={connected} lang="en" error={null} now={now} />)).toBe('')
  })
})
