//! Embedded Inter 4.1 subsets (`app/assets/fonts`, SIL OFL 1.1, see `OFL.txt`).
//!
//! egui's `default_fonts` feature is off: these three faces are the only fonts.
//! The subsets keep Basic Latin, Latin-1, Œ/œ, curly quotes, … · ∞ ✓ ★ ⚠ →
//! and the `kern`, `liga`, `calt` and `tnum` layout features.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const REGULAR: &str = "inter";
const SEMIBOLD: &str = "inter-semibold";
const BOLD: &str = "inter-bold";

/// `(family name, font bytes)`; the family name is also the font data key.
pub(crate) const FONTS: [(&str, &[u8]); 3] = [
    (
        REGULAR,
        include_bytes!("../../assets/fonts/Inter-Regular.subset.ttf"),
    ),
    (
        SEMIBOLD,
        include_bytes!("../../assets/fonts/Inter-SemiBold.subset.ttf"),
    ),
    (
        BOLD,
        include_bytes!("../../assets/fonts/Inter-Bold.subset.ttf"),
    ),
];

/// Registers the families `inter`, `inter-semibold` and `inter-bold` and makes
/// `Proportional` and `Monospace` use `inter` (egui's debug paths panic on an empty family).
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    for (name, bytes) in FONTS {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        defs.families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
    }
    defs.families
        .insert(FontFamily::Proportional, vec![REGULAR.to_owned()]);
    defs.families
        .insert(FontFamily::Monospace, vec![REGULAR.to_owned()]);
    ctx.set_fonts(defs);
}

/// Inter Regular (CSS weight 400).
pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(REGULAR.into()))
}

/// Inter SemiBold (CSS weights 600 and 650).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Inter Bold (CSS weights 700, 750 and 800).
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// The face for a CSS `font-weight`: up to 500 regular, up to 650 semibold, above bold.
pub fn weight(css_weight: u16, size: f32) -> FontId {
    match css_weight {
        0..=500 => regular(size),
        501..=650 => semibold(size),
        _ => bold(size),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::ui::i18n::{self, EN, FR};

    /// Every EN and FR string, parametrized ones called with sample arguments.
    fn all_ui_text() -> String {
        [&EN, &FR]
            .into_iter()
            .flat_map(i18n::all)
            .map(|(_, v)| v)
            .collect()
    }

    #[test]
    fn embedded_fonts_cover_every_ui_string() {
        // Every char of every EN/FR string (call fn fields with sample args) plus the ring symbols.
        let mut chars: BTreeSet<char> = all_ui_text().chars().collect();
        chars.extend(['∞', '✓', '★', '⚠', '·', '…', '’', '%']);
        // Digits and separators of numbers, times and durations ("94", "14:10", "2 h 05").
        chars.extend("0123456789:.,-".chars());
        for (name, bytes) in FONTS {
            let face = ttf_parser::Face::parse(bytes, 0).unwrap();
            let missing: Vec<_> = chars
                .iter()
                .filter(|c| !c.is_whitespace() && face.glyph_index(**c).is_none())
                .collect();
            assert!(missing.is_empty(), "{name} lacks {missing:?}");
        }
    }

    #[test]
    fn css_weights_map_to_the_three_faces() {
        assert_eq!(weight(400, 11.0), regular(11.0));
        assert_eq!(weight(600, 11.0), semibold(11.0));
        assert_eq!(weight(650, 11.0), semibold(11.0));
        assert_eq!(weight(700, 13.0), bold(13.0));
        assert_eq!(weight(750, 13.0), bold(13.0));
        assert_eq!(weight(800, 13.0), bold(13.0));
    }

    #[test]
    fn install_registers_the_families_and_proportional() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut out = ctx.run_pass(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let width = |font: FontId| {
            ctx.fonts_mut(|f| f.layout_no_wrap("Weekly 94%".into(), font, egui::Color32::WHITE))
                .size()
                .x
        };
        let (r, s, b) = (
            width(regular(12.0)),
            width(semibold(12.0)),
            width(bold(12.0)),
        );
        assert!(
            r > 0.0 && r < s && s < b,
            "regular {r}, semibold {s}, bold {b}"
        );
        assert_eq!(width(FontId::proportional(12.0)), r);
        // egui's debug paths use Monospace: an empty family would panic.
        assert_eq!(width(FontId::monospace(12.0)), r);
    }
}
