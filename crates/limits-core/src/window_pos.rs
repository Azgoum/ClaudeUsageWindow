#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Keep the saved position if the widget overlaps some monitor by at least 40 px in both
/// width and height; otherwise snap to the primary monitor's top-right (16 px margin).
pub fn clamp_position(
    saved: (i32, i32),
    size: (i32, i32),
    monitors: &[Rect],
    primary: Rect,
) -> (i32, i32) {
    const MIN_VISIBLE: i32 = 40;
    const MARGIN: i32 = 16;
    let visible = monitors.iter().any(|m| {
        let w = (saved.0 + size.0).min(m.x + m.w) - saved.0.max(m.x);
        let h = (saved.1 + size.1).min(m.y + m.h) - saved.1.max(m.y);
        w >= MIN_VISIBLE && h >= MIN_VISIBLE
    });
    if visible {
        saved
    } else {
        (primary.x + primary.w - size.0 - MARGIN, primary.y + MARGIN)
    }
}

/// Window bounds while a popup (tooltip, menu) needs room beyond the widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub pos: (i32, i32),
    pub size: (i32, i32),
    /// How far below the window's top edge the widget sits (non-zero when grown upward).
    pub offset_y: i32,
}

/// Grows the window by `extra` px under a widget resting at `rest` with size `base`.
/// Downward when there is room on the work area, else upward (the window moves up and the
/// widget sits `offset_y` lower inside it); when neither fits, toward the larger side.
pub fn expand_bounds(rest: (i32, i32), base: (i32, i32), extra: i32, work: Rect) -> Bounds {
    let size = (base.0, base.1 + extra.max(0));
    let below = work.y + work.h - (rest.1 + base.1);
    let above = rest.1 - work.y;
    let up = if below >= extra {
        0
    } else if above >= extra {
        extra
    } else if above > below {
        above.max(0)
    } else {
        0
    };
    Bounds { pos: (rest.0, rest.1 - up), size, offset_y: up }
}

/// Remembers the widget's resting position while the window is temporarily expanded.
#[derive(Debug, Default)]
pub struct PopupRoom {
    /// Resting position and the bounds last applied, while expanded.
    state: Option<((i32, i32), Bounds)>,
}

impl PopupRoom {
    /// True while expanded: window moves must not be persisted.
    pub fn expanded(&self) -> bool {
        self.state.is_some()
    }

    /// New window bounds for `extra` px of popup room (0 = back to rest).
    pub fn apply(&mut self, current: (i32, i32), base: (i32, i32), extra: i32, work: Rect) -> Bounds {
        let rest = match self.state {
            // Moved by someone else (a drag) while expanded: the widget moved with the window.
            Some((_, last)) if last.pos != current => (current.0, current.1 + last.offset_y),
            Some((rest, _)) => rest,
            None => current,
        };
        if extra <= 0 {
            self.state = None;
            return Bounds { pos: rest, size: base, offset_y: 0 };
        }
        let b = expand_bounds(rest, base, extra, work);
        self.state = Some((rest, b));
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Rect = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    const SIZE: (i32, i32) = (320, 200);

    #[test]
    fn onscreen_kept() {
        assert_eq!(clamp_position((100, 100), SIZE, &[PRIMARY], PRIMARY), (100, 100));
    }

    #[test]
    fn partially_offscreen_kept_if_at_least_40px_visible() {
        // 40 px visible horizontally (x = 1880), fully visible vertically.
        assert_eq!(clamp_position((1880, 100), SIZE, &[PRIMARY], PRIMARY), (1880, 100));
        // Negative coordinates, 40 px visible on the left edge.
        assert_eq!(clamp_position((-280, -160), SIZE, &[PRIMARY], PRIMARY), (-280, -160));
        // Only 39 px visible horizontally: snapped.
        assert_eq!(clamp_position((1881, 100), SIZE, &[PRIMARY], PRIMARY), (1584, 16));
        // Visible horizontally but only 39 px vertically: snapped.
        assert_eq!(clamp_position((100, 1041), SIZE, &[PRIMARY], PRIMARY), (1584, 16));
    }

    #[test]
    fn offscreen_snaps_to_primary_top_right() {
        let primary = Rect { x: 100, y: 50, w: 1920, h: 1080 };
        let got = clamp_position((5000, 5000), SIZE, &[primary], primary);
        assert_eq!(got, (primary.x + primary.w - SIZE.0 - 16, primary.y + 16));
    }

    #[test]
    fn visible_on_secondary_monitor_kept() {
        let second = Rect { x: 1920, y: 0, w: 1280, h: 1024 };
        assert_eq!(clamp_position((2500, 300), SIZE, &[PRIMARY, second], PRIMARY), (2500, 300));
    }

    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };
    const BASE: (i32, i32) = (244, 146);

    #[test]
    fn grows_down_when_room_below() {
        let b = expand_bounds((100, 100), BASE, 120, WORK);
        assert_eq!(b, Bounds { pos: (100, 100), size: (244, 266), offset_y: 0 });
    }

    #[test]
    fn grows_up_when_no_room_below() {
        // Bottom of the widget at 1000, only 40 px left above the 1040 work-area bottom.
        let b = expand_bounds((100, 854), BASE, 120, WORK);
        assert_eq!(b, Bounds { pos: (100, 734), size: (244, 266), offset_y: 120 });
    }

    #[test]
    fn neither_fits_uses_the_larger_side() {
        let small = Rect { x: 0, y: 0, w: 800, h: 400 };
        // 60 px above, 194 below: grow down (partly off the work area).
        let b = expand_bounds((10, 60), BASE, 300, small);
        assert_eq!(b, Bounds { pos: (10, 60), size: (244, 446), offset_y: 0 });
        // 200 px above, 54 below: grow up as far as the work-area top.
        let b = expand_bounds((10, 200), BASE, 300, small);
        assert_eq!(b, Bounds { pos: (10, 0), size: (244, 446), offset_y: 200 });
    }

    #[test]
    fn popup_room_restores_the_resting_position() {
        let mut room = PopupRoom::default();
        assert!(!room.expanded());
        let up = room.apply((100, 854), BASE, 120, WORK);
        assert_eq!(up.pos, (100, 734));
        assert!(room.expanded());
        // A second popup measured while expanded: still computed from the rest, not the moved window.
        let up2 = room.apply(up.pos, BASE, 90, WORK);
        assert_eq!(up2, Bounds { pos: (100, 764), size: (244, 236), offset_y: 90 });
        let back = room.apply(up2.pos, BASE, 0, WORK);
        assert_eq!(back, Bounds { pos: (100, 854), size: BASE, offset_y: 0 });
        assert!(!room.expanded());
        // Not expanded: a base-size change keeps the current position.
        let tall = room.apply((300, 40), (244, 168), 0, WORK);
        assert_eq!(tall, Bounds { pos: (300, 40), size: (244, 168), offset_y: 0 });
    }

    #[test]
    fn popup_room_follows_a_window_dragged_while_expanded() {
        let mut room = PopupRoom::default();
        let up = room.apply((100, 854), BASE, 120, WORK); // window at (100, 734), widget 120 px down
        assert_eq!(up.pos, (100, 734));
        // The user drags the expanded window by (+50, -400): the rest moves with it.
        let back = room.apply((150, 334), BASE, 0, WORK);
        assert_eq!(back, Bounds { pos: (150, 454), size: BASE, offset_y: 0 });
        // Same while staying expanded: recomputed from the new rest (room below now).
        room.apply((100, 854), BASE, 120, WORK);
        let again = room.apply((400, 100), BASE, 120, WORK);
        assert_eq!(again, Bounds { pos: (400, 220), size: (244, 266), offset_y: 0 });
    }
}
