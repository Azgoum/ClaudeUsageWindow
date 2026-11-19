//! Popup room (port of Phase 1 (git bcaa1bf) `ui/src/lib/room.ts`): extra window height the
//! open context menu needs.

/// Collision padding, points.
pub const PAD: f32 = 6.0;

/// Rounds away float noise (f32 sums like 92.00001) before `ceil`.
fn ceil_clean(v: f32) -> f32 {
    ((v * 1000.0).round() / 1000.0).ceil()
}

/// Context menu: opens at the cursor (`cursor_y`, relative to the widget top) and goes down.
pub fn menu_room(cursor_y: f32, menu_h: f32, widget_h: f32) -> f32 {
    ceil_clean(cursor_y + menu_h + PAD - widget_h).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_room_is_below_cursor_plus_padding_beyond_widget() {
        assert_eq!(menu_room(40.0, 257.0, 145.78), 158.0);
    }

    #[test]
    fn never_negative() {
        assert_eq!(menu_room(0.0, 50.0, 145.78), 0.0);
    }
}
