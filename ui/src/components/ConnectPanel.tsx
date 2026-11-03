import { openUrl, send } from '@/lib/bridge'
import { t } from '@/lib/i18n'
import type { Conn, Lang } from '@/lib/types'

const FIREFOX_URL = 'https://www.mozilla.org/firefox/'

interface Props {
  conn: Conn
  lang: Lang
}

/** Everything the widget shows while it is not connected (prototype `.st` block). */
export function ConnectPanel({ conn, lang }: Props) {
  const L = t(lang)
  const connect = () => void send({ type: 'connect' })

  switch (conn.state) {
    case 'connected':
      return null // connected but no rings yet: the next state event brings them
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
