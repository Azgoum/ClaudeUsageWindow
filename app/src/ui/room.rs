//! Popup room (port of `ui/src/lib/room.ts`): extra window height an open popup needs.

use limits_core::LimitKey;

/// Collision padding, points.
pub const PAD: f32 = 6.0;

/// Rounds away float noise (f32 sums like 92.00001) before `ceil`.
fn ceil_clean(v: f32) -> f32 {
    ((v * 1000.0).round() / 1000.0).ceil()
}

/// Ring tooltip: placed 16 under the ring trigger (`trigger_bottom`, relative to the widget top).
pub fn tip_room(trigger_bottom: f32, tip_h: f32, widget_h: f32) -> f32 {
    ceil_clean(trigger_bottom + 16.0 + tip_h + PAD - widget_h).max(0.0)
}

/// Context menu: opens at the cursor (`cursor_y`, relative to the widget top) and goes down.
pub fn menu_room(cursor_y: f32, menu_h: f32, widget_h: f32) -> f32 {
    ceil_clean(cursor_y + menu_h + PAD - widget_h).max(0.0)
}

/// Per-ring tooltip room; each ring's tooltip reports (and clears) only its own slot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RingRooms {
    pub session: Option<f32>,
    pub weekly: Option<f32>,
}

pub fn with_ring_room(rooms: RingRooms, key: LimitKey, room: f32) -> RingRooms {
    let mut out = rooms;
    match key {
        LimitKey::Session => out.session = Some(room),
        LimitKey::Weekly => out.weekly = Some(room),
    }
    out
}

pub fn max_room(rooms: &RingRooms) -> f32 {
    [rooms.session, rooms.weekly]
        .into_iter()
        .flatten()
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Widget 145.78 tall, ring trigger bottom at 122.78.
    #[test]
    fn tooltip_room_is_bottom_plus_gap_height_padding_beyond_widget() {
        assert_eq!(tip_room(122.78, 93.0, 145.78), 92.0);
    }

    #[test]
    fn menu_room_is_below_cursor_plus_padding_beyond_widget() {
        assert_eq!(menu_room(40.0, 257.0, 145.78), 158.0);
    }

    #[test]
    fn never_negative() {
        assert_eq!(tip_room(10.0, 20.0, 145.78), 0.0);
        assert_eq!(menu_room(0.0, 50.0, 145.78), 0.0);
    }

    #[test]
    fn moving_between_ring_tooltips_keeps_the_open_ones_room() {
        let mut rooms = with_ring_room(RingRooms::default(), LimitKey::Session, 92.0);
        rooms = with_ring_room(rooms, LimitKey::Weekly, 92.0); // B mounts first...
        rooms = with_ring_room(rooms, LimitKey::Session, 0.0); // ...A unmounts after its fade
        assert_eq!(max_room(&rooms), 92.0);
        assert_eq!(max_room(&with_ring_room(rooms, LimitKey::Weekly, 0.0)), 0.0);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(max_room(&RingRooms::default()), 0.0);
    }
}
