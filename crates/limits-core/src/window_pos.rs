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
}
