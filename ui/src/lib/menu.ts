import type { Conn } from './types'

/** Refresh, Organization and Disconnect only make sense while connected. */
export function menuShows(conn: Conn): { connectedItems: boolean } {
  return { connectedItems: conn.state === 'connected' }
}
