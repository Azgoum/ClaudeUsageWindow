//! Renderer spike 3: winit + egui without eframe. egui's meshes are rasterized
//! on the CPU (`limitrings_app::render`) into a premultiplied BGRA DIB and shown
//! with `UpdateLayeredWindow` on a `WS_EX_LAYERED` window
//! (`limitrings_app::platform::Surface`): true per-pixel alpha, no GPU.
//! The event loop is `limitrings_app::host::run`.

#[path = "spike_common/mod.rs"]
mod spike_common;

use limitrings_app::host::{run, WindowSpec};
use spike_common::{Scene, POSITION, TITLE};

fn main() {
    let spec = WindowSpec {
        title: TITLE,
        size: limitrings_app::WINDOW_SIZE,
        position: Some(POSITION),
    };
    let mut scene = Scene::default();
    run(spec, |ctx| scene.draw(ctx));
}
