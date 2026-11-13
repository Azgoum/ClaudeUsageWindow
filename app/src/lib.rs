//! Claude Limit Checker: native egui widget.

pub mod anim;
pub mod host;
pub mod launch;
pub mod logging;
pub mod platform;
pub mod render;
pub mod shell;
pub mod ui;

use egui::{LayerId, Vec2};

/// Window title (display name, see Global Constraints).
pub const TITLE: &str = "Claude Limit Checker";
/// Logical size of the widget window.
pub const WINDOW_SIZE: Vec2 = Vec2::new(244.0, 146.0);

/// Opens the widget window and blocks until it closes.
pub fn run() {
    let spec = host::WindowSpec {
        title: TITLE,
        size: WINDOW_SIZE,
        position: None,
    };
    host::run(spec, |ctx| {
        ui::draw::glass(
            &ctx.layer_painter(LayerId::background()),
            ctx.content_rect(),
            false,
        );
    });
}
