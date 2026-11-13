//! Claude Limit Checker: native egui widget.

pub mod anim;
pub mod host;
pub mod launch;
pub mod logging;
pub mod platform;
pub mod render;
pub mod shell;
pub mod ui;

use egui::{CornerRadius, LayerId, Pos2, Rect, Stroke, StrokeKind, Vec2};

/// Window title (display name, see Global Constraints).
pub const TITLE: &str = "Claude Limit Checker";
/// Logical size of the widget window.
pub const WINDOW_SIZE: Vec2 = Vec2::new(244.0, 146.0);

/// Glass values from the Global Constraints.
pub mod glass {
    use egui::Color32;

    pub const RADIUS: u8 = 16;
    pub const FILL: Color32 = Color32::from_rgba_unmultiplied_const(22, 22, 28, 204);
    pub const BORDER: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 23);
    pub const HIGHLIGHT: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 15);
}

/// Paints the glass panel filling `rect`: fill, 1 px inner border, 1 px top inner highlight.
pub fn paint_glass(painter: &egui::Painter, rect: Rect) {
    painter.rect(
        rect,
        CornerRadius::same(glass::RADIUS),
        glass::FILL,
        Stroke::new(1.0, glass::BORDER),
        StrokeKind::Inside,
    );
    // `inset 0 1px 0`: a 1 px band just under the top border, between the corner arcs.
    let r = f32::from(glass::RADIUS);
    let y = rect.top() + 1.5;
    painter.line_segment(
        [
            Pos2::new(rect.left() + r, y),
            Pos2::new(rect.right() - r, y),
        ],
        Stroke::new(1.0, glass::HIGHLIGHT),
    );
}

/// Opens the widget window and blocks until it closes.
pub fn run() {
    let spec = host::WindowSpec {
        title: TITLE,
        size: WINDOW_SIZE,
        position: None,
    };
    host::run(spec, |ctx| {
        paint_glass(
            &ctx.layer_painter(LayerId::background()),
            ctx.content_rect(),
        );
    });
}
