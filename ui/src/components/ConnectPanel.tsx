import { openUrl, send } from '@/lib/bridge'
import { t } from '@/lib/i18n'
import { errLine } from '@/lib/ring'
import type { AppState, Conn, Lang } from '@/lib/types'

const FIREFOX_URL = 'https://www.mozilla.org/firefox/'

interface Props {
  conn: Conn
  lang: Lang
  /** Connected but no rings yet: the failed first poll's retry time, if any. */
  error: AppState['error']
  now: number
}

/** Everything the widget shows while it is not connected (prototype `.st` block). */
export function ConnectPanel({ conn, lang, error, now }: Props) {
  const L = t(lang)
  const connect = () => void send({ type: 'connect' })

  switch (conn.state) {
    case 'connected':
      // No rings yet. Empty while the first poll runs; after a failure (e.g. no network at
      // logon), the same retry line the rings show.
      return error ? (
        <div className="st" data-conn="connected">
          <p className="msg">{errLine(error.retryAt, now, lang)}</p>
        </div>
      ) : null
    case 'waiting':
      return (
        <div className="st" data-conn="waiting">
          <h4>
            <span className="spin" aria-hidden="true" />
            {L.wTitle}
          </h4>
          <p>{L.wOther('Firefox')}</p>
          <div className="lnks">
            <button type="button" className="lnk" onClick={connect}>
              {L.again}
            </button>
            <button type="button" className="lnk" onClick={() => void send({ type: 'cancelConnect' })}>
              {L.cancel}
            </button>
          </div>
        </div>
      )
    case 'needsFirefox':
      return (
        <div className="st" data-conn="needsFirefox">
          <h4>{L.needFirefoxTitle}</h4>
          <p>{L.needFirefoxText}</p>
          <button type="button" className="btn-c" onClick={() => void openUrl(FIREFOX_URL)}>
            {L.getFirefox}
          </button>
        </div>
      )
    case 'expired':
      return (
        <div className="st" data-conn="expired">
          <h4>{L.eTitle}</h4>
          <p>{L.eText}</p>
          <button type="button" className="btn-c" onClick={connect}>
            {L.eBtn}
          </button>
        </div>
      )
    case 'disconnected':
      return (
        <div className="st" data-conn="disconnected">
          <h4>{L.cTitle}</h4>
          <p>{conn.timedOut ? L.timedOut : L.cText}</p>
          <button type="button" className="btn-c" onClick={connect}>
            {L.cBtn}
          </button>
        </div>
      )
  }
}
