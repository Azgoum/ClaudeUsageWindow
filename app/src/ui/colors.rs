//! Palette (port of `ui/src/lib/colors.ts` plus every color the widget tasks name).

use egui::Color32;
use limits_core::Pace;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
/// Straight (non-premultiplied) RGBA, alpha 0..=255.
const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

pub const GREEN: Color32 = rgb(0x34, 0xd3, 0x99);
pub const RED: Color32 = rgb(0xfb, 0x71, 0x85);
pub const BLUE: Color32 = rgb(0x7d, 0xd3, 0xfc);
pub const IDLE: Color32 = rgb(0x5f, 0x5f, 0x69);
pub const GOLD: Color32 = rgb(0xfa, 0xcc, 0x15);
pub const INK: Color32 = rgb(0xe7, 0xe7, 0xea);
pub const MUTE: Color32 = rgb(0x8b, 0x8b, 0x95);

/// Ring name (11 px semibold).
pub const NAME: Color32 = rgb(0xc9, 0xc9, 0xd1);
/// Error foot line and error body text.
pub const ERR_TEXT: Color32 = rgb(0xfd, 0xa4, 0xaf);
/// Connect button gradient, top to bottom.
pub const BTN_FROM: Color32 = rgb(0xf3, 0xc3, 0xa9);
pub const BTN_TO: Color32 = rgb(0xd9, 0x77, 0x57);
pub const BTN_LABEL: Color32 = rgb(0x1a, 0x1a, 0x1a);
pub const BTN_SHADOW: Color32 = rgba(217, 119, 87, 89);
/// Links ("open claude.ai again", "cancel") and menu check marks.
pub const LINK: Color32 = rgb(0xf0, 0xb3, 0x8f);
/// Window close button hover.
pub const CLOSE_HOVER: Color32 = rgb(0xe8, 0x11, 0x23);

/// Ring track `rgba(255,255,255,.08)`.
pub const TRACK: Color32 = rgba(255, 255, 255, 20);
/// Arc of a ring without limit `rgba(255,255,255,.18)`.
pub const NO_LIMIT_ARC: Color32 = rgba(255, 255, 255, 46);
/// Control button hover `rgba(255,255,255,.09)`.
pub const BTN_HOVER_BG: Color32 = rgba(255, 255, 255, 23);
/// Flag outlines: inactive `.15`, active `.75`.
pub const FLAG_OUTLINE: Color32 = rgba(255, 255, 255, 38);
pub const FLAG_OUTLINE_ACTIVE: Color32 = rgba(255, 255, 255, 191);

/// Context menu.
pub const MENU_BG: Color32 = rgba(32, 32, 38, 245);
pub const MENU_BORDER: Color32 = rgba(255, 255, 255, 26);
pub const MENU_HOVER: Color32 = rgba(255, 255, 255, 20);
pub const MENU_SEP: Color32 = rgba(255, 255, 255, 20);
pub const MENU_SHADOW: Color32 = rgba(0, 0, 0, 128);

/// Surprise glow `rgba(250,204,21,.32)` and its ring `rgba(250,204,21,.85)`.
pub const GLOW_INNER: Color32 = rgba(250, 204, 21, 82);
pub const GLOW_RING: Color32 = rgba(250, 204, 21, 217);
/// Spark colors, cycled.
pub const SPARKS: [Color32; 4] = [
    rgb(0xfa, 0xcc, 0x15),
    rgb(0xfd, 0xe6, 0x8a),
    rgb(0xfb, 0x92, 0x3c),
    rgb(0xff, 0xff, 0xff),
];

/// Reset pill, calm variant.
pub const PILL_CALM_BG: Color32 = rgba(20, 40, 32, 230);
pub const PILL_CALM_TEXT: Color32 = rgb(0xa7, 0xf3, 0xd0);
pub const PILL_CALM_BORDER: Color32 = rgba(52, 211, 153, 115);
/// Reset pill, gold (surprise) variant.
pub const PILL_GOLD_BG: Color32 = rgba(48, 40, 10, 235);
pub const PILL_GOLD_TEXT: Color32 = rgb(0xfd, 0xe6, 0x8a);
pub const PILL_GOLD_BORDER: Color32 = rgba(250, 204, 21, 140);
pub const PILL_GOLD_GLOW: Color32 = rgba(250, 204, 21, 77);

pub fn pace_color(p: Pace) -> Color32 {
    match p {
        Pace::Ahead => RED,
        Pace::OnTrack => GREEN,
        Pace::Room => BLUE,
        Pace::Idle => IDLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_colors_match_phase_1() {
        assert_eq!(pace_color(Pace::Ahead), Color32::from_rgb(0xfb, 0x71, 0x85));
        assert_eq!(
            pace_color(Pace::OnTrack),
            Color32::from_rgb(0x34, 0xd3, 0x99)
        );
        assert_eq!(pace_color(Pace::Room), Color32::from_rgb(0x7d, 0xd3, 0xfc));
        assert_eq!(pace_color(Pace::Idle), Color32::from_rgb(0x5f, 0x5f, 0x69));
        assert_eq!(GOLD, Color32::from_rgb(0xfa, 0xcc, 0x15));
    }
}
