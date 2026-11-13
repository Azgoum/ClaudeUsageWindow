//! The window host: a winit event loop driving one `egui::Context`, rendered on
//! the CPU ([`crate::render`]) and shown through [`crate::platform::Surface`]
//! (a layered window on Windows). Repaints only when egui asks for one (input,
//! animation, `request_repaint_after`), so an idle widget costs no CPU.

use std::{sync::Arc, time::Instant};

use egui::{Vec2, ViewportId};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId, WindowLevel},
};

use crate::{platform, render::SoftRenderer};

/// What the host window looks like when it opens.
pub struct WindowSpec {
    pub title: &'static str,
    /// Logical size in points.
    pub size: Vec2,
    /// Logical screen position; `None` lets Windows choose.
    pub position: Option<[f32; 2]>,
}

/// Opens a borderless, transparent, non-resizable, always-on-top window and
/// calls `ui` every time egui needs a frame. Returns when the window closes.
pub fn run(spec: WindowSpec, ui: impl FnMut(&egui::Context)) {
    let event_loop = EventLoop::new().expect("winit event loop");
    let mut host = Host {
        spec,
        ui,
        ctx: egui::Context::default(),
        running: None,
    };
    event_loop.run_app(&mut host).expect("winit event loop");
}

struct Host<F> {
    spec: WindowSpec,
    ui: F,
    ctx: egui::Context,
    running: Option<Running>,
}

struct Running {
    window: Arc<Window>,
    input: egui_winit::State,
    renderer: SoftRenderer,
    surface: platform::Surface,
    next_repaint: Option<Instant>,
}

impl<F: FnMut(&egui::Context)> ApplicationHandler for Host<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title(self.spec.title)
            .with_inner_size(LogicalSize::new(self.spec.size.x, self.spec.size.y))
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_visible(false);
        if let Some([x, y]) = self.spec.position {
            attrs = attrs.with_position(LogicalPosition::new(x, y));
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let input = egui_winit::State::new(
            self.ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let mut running = Running {
            window: window.clone(),
            input,
            renderer: SoftRenderer::default(),
            surface: platform::surface_for(&window),
            next_repaint: None,
        };
        // Paint before showing so the window never flashes empty.
        running.paint(&self.ctx, &mut self.ui);
        window.set_visible(true);
        self.running = Some(running);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => running.paint(&self.ctx, &mut self.ui),
            event => {
                if running
                    .input
                    .on_window_event(&running.window, &event)
                    .repaint
                {
                    running.window.request_redraw();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let flow = match running.next_repaint {
            Some(at) if at <= Instant::now() => {
                running.next_repaint = None;
                running.window.request_redraw();
                ControlFlow::Wait
            }
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        };
        event_loop.set_control_flow(flow);
    }
}

impl Running {
    fn paint(&mut self, ctx: &egui::Context, ui: &mut impl FnMut(&egui::Context)) {
        let raw_input = self.input.take_egui_input(&self.window);
        let output = ctx.run_pass(raw_input, |ctx| ui(ctx));
        self.input
            .handle_platform_output(&self.window, output.platform_output);

        let ppp = output.pixels_per_point;
        let primitives = ctx.tessellate(output.shapes, ppp);
        let size = self.window.inner_size();
        let (width, height) = (size.width as usize, size.height as usize);
        let pixels = self.surface.pixels(width, height);
        self.renderer.render(
            pixels,
            width,
            height,
            &primitives,
            output.textures_delta,
            ppp,
        );
        if let Err(err) = self.surface.present() {
            eprintln!("presenting the window failed: {err}");
        }

        let delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        self.next_repaint = Instant::now().checked_add(delay);
    }
}
