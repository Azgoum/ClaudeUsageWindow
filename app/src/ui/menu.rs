//! Context-menu model (port of `ui/src/lib/menu.ts`).

use limits_core::Conn;

/// Refresh, Organization and Disconnect only make sense while connected.
pub fn connected_items(conn: &Conn) -> bool {
    matches!(conn, Conn::Connected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_shows_refresh_orgs_and_disconnect() {
        assert!(connected_items(&Conn::Connected));
    }

    #[test]
    fn other_states_only_show_top_boot_quit() {
        for conn in [
            Conn::Disconnected { timed_out: false },
            Conn::Disconnected { timed_out: true },
            Conn::Waiting,
            Conn::NeedsFirefox,
            Conn::Expired,
        ] {
            assert!(!connected_items(&conn), "{conn:?}");
        }
    }
}
