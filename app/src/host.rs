//! The window host: a winit event loop driving the widget's `egui::Context`,
//! rendered on the CPU ([`crate::render`]) and shown through
//! [`crate::platform::Surface`] (a layered window on Windows).
//!
//! Repaints only when something asks for one: input, an engine state change
//! (the engine thread wakes the loop), the widget's [`Repaint`] policy or egui's
//! own animations. A pending position save wakes the loop once, 500 ms later.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{Local, Offset, Utc};
use egui::{Vec2, ViewportId};
use limits_core::{persisted_position, AppState, Command, PopupRoom, Rect, SettingsStore};
use tokio::sync::{mpsc, watch};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Icon, Window, WindowAttributes, WindowId, WindowLevel},
};

use crate::{
    platform,
    render::SoftRenderer,
    shell,
    ui::{self, widget::Widget, Action, FrameInput, Lang, Prefs, Repaint},
};

/// Debounce of the saved window position (as Phase 1).
pub const MOVE_DEBOUNCE: Duration = Duration::from_millis(500);
/// If the normal exit hasn't ended the process after this long, it is ended
/// outright: Quit must never leave an invisible process behind.
const EXIT_FALLBACK: Duration = Duration::from_secs(3);

/// `v` points to whole pixels, rounding up; float noise (145.6 Ã— 1.25 =
/// 182.00001) is rounded away first.
fn px(v: f32, ppp: f32) -> i32 {
    ((v.max(0.0) * ppp * 1000.0).round() / 1000.0).ceil() as i32
}

/// The widget box (`size`) and the popup room below it (`extra`), points, in
/// physical pixels: `(base, extra)`, each side rounded up.
pub fn fit_bounds(ppp: f32, size: Vec2, extra: f32) -> ((i32, i32), i32) {
    ((px(size.x, ppp), px(size.y, ppp)), px(extra, ppp))
}

/// Debounces window moves into one position save, 500 ms after the last move.
/// Minimizing (Windows parks the window at -32000) and the host's own moves
/// while a popup has the window expanded are not moves.
#[derive(Debug, Default)]
pub struct MoveSaver {
    /// Last resting position seen; the first observation only sets it.
    last: Option<(i32, i32)>,
    /// Last position saved (or the first one seen): never saved again.
    saved: Option<(i32, i32)>,
    /// Position to save and when.
    pending: Option<((i32, i32), Instant)>,
}

impl MoveSaver {
    pub fn observe(&mut self, pos: (i32, i32), now: Instant, minimized: bool, expanded: bool) {
        if minimized || expanded {
            return;
        }
        match self.last.replace(pos) {
            Some(last) if last != pos => self.pending = Some((pos, now + MOVE_DEBOUNCE)),
            Some(_) => {}
            None => self.saved = Some(pos),
        }
    }

    /// The position to save once its debounce has elapsed (once); `None` when
    /// the window came back to where it was last saved.
    pub fn due(&mut self, now: Instant) -> Option<(i32, i32)> {
        match self.pending {
            Some((pos, at)) if at <= now => {
                self.pending = None;
                (self.saved.replace(pos) != Some(pos)).then_some(pos)
            }
            _ => None,
        }
    }

    /// When the pending save is due.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.map(|(_, at)| at)
    }
}

/// Wakes the event loop from the engine thread.
#[derive(Debug, Clone, Copy)]
pub struct Wake;

/// What the host needs from the shell.
pub struct Shell {
    pub cmd: mpsc::Sender<Command>,
    pub state: watch::Receiver<AppState>,
    /// A clone of the engine's store: clones share the write lock.
    pub settings: SettingsStore,
    pub prefs: Prefs,
    /// See [`shell::manages_autostart`]: debug exes never touch the Run value.
    pub manage_autostart: bool,
    /// Saved position, physical pixels.
    pub saved_position: Option<(i32, i32)>,
}

/// The event loop the engine's `wake` closure needs a proxy of.
pub fn event_loop() -> EventLoop<Wake> {
    EventLoop::with_user_event()
        .build()
        .expect("winit event loop")
}

/// Opens the widget window and blocks until it closes.
pub fn run(event_loop: EventLoop<Wake>, shell: Shell) {
    let ctx = egui::Context::default();
    ui::fonts::install(&ctx);
    let mut host = Host {
        shell,
        ctx,
        widget: Widget::new(),
        reduced_motion: platform::reduced_motion(),
        running: None,
    };
    event_loop.run_app(&mut host).expect("winit event loop");
}

struct Host {
    shell: Shell,
    ctx: egui::Context,
    widget: Widget,
    reduced_motion: bool,
    running: Option<Running>,
}

struct Running {
    window: Arc<Window>,
    hwnd: Option<isize>,
    input: egui_winit::State,
    renderer: SoftRenderer,
    surface: platform::Surface,
    next_repaint: Option<Instant>,
    focused: bool,
    /// Points the widget sits below the window top (window grown upward).
    offset_y: f32,
    room: PopupRoom,
    saver: MoveSaver,
    /// Last presentation error, logged once until it changes.
    present_error: Option<String>,
    /// When the host last moved or resized the window itself.
    reshaped_at: Option<Instant>,
}

/// How long after the host moved or resized the window a "pointer left" report
/// is checked against the real cursor (see [`platform::spurious_leave`]).
const SPURIOUS_LEAVE_WINDOW: Duration = Duration::from_millis(300);

/// Widget passes per frame: one, plus one per window change it causes (popup
/// room, then a settle).
const PASSES: usize = 3;

/// A window's position and size, physical pixels.
type Placement = ((i32, i32), (i32, i32));

/// What a frame asks of the event loop once it is on screen.
#[derive(Default)]
struct After {
    drag: bool,
    quit: bool,
}

fn window_icon() -> Option<Icon> {
    let rgba = include_bytes!(concat!(env!("OUT_DIR"), "/icon-64.rgba"));
    Icon::from_rgba(rgba.to_vec(), 64, 64)
        .inspect_err(|e| tracing::warn!("bad window icon: {e}"))
        .ok()
}

fn window_attributes(always_on_top: bool) -> WindowAttributes {
    let attrs = Window::default_attributes()
        .with_title(crate::TITLE)
        .with_inner_size(LogicalSize::new(crate::WINDOW_SIZE.x, crate::WINDOW_SIZE.y))
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(level(always_on_top))
        .with_window_icon(window_icon())
        .with_visible(false);
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs
            .with_class_name(platform::WINDOW_CLASS)
            .with_taskbar_icon(window_icon())
            .with_undecorated_shadow(false)
            // No file drops: skips OLE drag-and-drop registration.
            .with_drag_and_drop(false)
    };
    attrs
}

fn level(always_on_top: bool) -> WindowLevel {
    if always_on_top {
        WindowLevel::AlwaysOnTop
    } else {
        WindowLevel::Normal
    }
}

fn quit(event_loop: &ActiveEventLoop, from: &'static str) {
    tracing::info!(from, "quit requested");
    event_loop.exit();
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_FALLBACK);
        tracing::warn!("exit still pending after 3 s; ending the process");
        std::process::exit(0);
    });
}

impl ApplicationHandler<Wake> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let attrs = window_attributes(self.shell.prefs.always_on_top);
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
            hwnd: platform::hwnd_of(&window),
            surface: platform::surface_for(&window),
            window: window.clone(),
            input,
            renderer: SoftRenderer::default(),
            next_repaint: None,
            focused: false,
            offset_y: 0.0,
            room: PopupRoom::default(),
            saver: MoveSaver::default(),
            present_error: None,
            reshaped_at: None,
        };
        // A first frame sizes the window to the widget; then it is placed (the
        // saved position, clamped to the monitors) and painted there before it
        // is shown, so it never flashes empty or elsewhere.
        running.frame(self);
        running.place(self.shell.saved_position);
        running.frame(self);
        window.set_visible(true);
        if let Some(rect) = running.rect() {
            running
                .saver
                .observe((rect.x, rect.y), Instant::now(), false, false);
        }
        self.running = Some(running);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _wake: Wake) {
        if let Some(running) = &self.running {
            running.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(mut running) = self.running.take() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => quit(event_loop, "close"),
            WindowEvent::RedrawRequested => {
                let after = running.frame(self);
                if after.drag {
                    if let Err(e) = running.window.drag_window() {
                        tracing::warn!("cannot start the window drag: {e}");
                    }
                }
                if after.quit {
                    quit(event_loop, "widget");
                }
            }
            WindowEvent::CursorLeft { .. }
                if running
                    .reshaped_at
                    .is_some_and(|at| at.elapsed() < SPURIOUS_LEAVE_WINDOW)
                    && running.hwnd.is_some_and(platform::spurious_leave) =>
            {
                // The window moved under a still cursor (a popup grew it upward).
            }
            WindowEvent::Moved(pos) => {
                let minimized = running.hwnd.is_some_and(platform::is_minimized);
                running.saver.observe(
                    (pos.x, pos.y),
                    Instant::now(),
                    minimized,
                    running.room.expanded(),
                );
            }
            event => {
                if let WindowEvent::Focused(focused) = event {
                    running.focused = focused;
                }
                let response = running.input.on_window_event(&running.window, &event);
                if response.repaint || matches!(event, WindowEvent::Focused(_)) {
                    running.window.request_redraw();
                }
            }
        }
        self.running = Some(running);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let now = Instant::now();
        if let Some(pos) = running.saver.due(now) {
            let pos = persisted_position(pos, &running.room);
            tracing::debug!(?pos, "saving the window position");
            if let Err(e) = self.shell.settings.update(|s| s.position = Some(pos)) {
                tracing::warn!("cannot save settings: {e}");
            }
        }
        if running.next_repaint.is_some_and(|at| at <= now) {
            running.next_repaint = None;
            running.window.request_redraw();
        }
        let wake = [running.next_repaint, running.saver.deadline()]
            .into_iter()
            .flatten()
            .min();
        event_loop.set_control_flow(match wake {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}

impl Running {
    fn rect(&self) -> Option<Rect> {
        platform::window_rect(self.hwnd?)
    }

    /// Moves the window to the saved position (clamped to the monitors) or the
    /// primary's top-right.
    fn place(&mut self, saved: Option<(i32, i32)>) {
        let (Some(hwnd), Some(rect)) = (self.hwnd, self.rect()) else {
            return;
        };
        let monitors = platform::work_areas();
        let Some(primary) = platform::primary_work_area().or(monitors.first().copied()) else {
            return;
        };
        let size = (rect.w, rect.h);
        let pos = shell::initial_position(saved, size, &monitors, primary);
        platform::set_window_bounds(hwnd, pos, size);
    }

    /// Runs the widget until the window fits it (at most 3 passes), then shows
    /// the last pass and schedules the next frame.
    ///
    /// The window's new position, size and pixels reach the screen in one
    /// `Surface::present`: moving or growing the window first would leave a
    /// transparent (click-through) gap under the cursor for an instant, enough
    /// for Windows to report the pointer gone and close the tooltip that asked
    /// for the room.
    fn frame(&mut self, host: &mut Host) -> After {
        let mut after = After::default();
        if self.hwnd.is_some_and(platform::is_minimized) {
            return after; // nothing to show; restoring sends the events that repaint
        }
        let ppp = self.window.scale_factor() as f32;
        let current = self.rect().map(|r| ((r.x, r.y), (r.w, r.h)));
        let mut target = current.unwrap_or_else(|| {
            let s = self.window.inner_size();
            ((0, 0), (s.width as i32, s.height as i32))
        });
        // How far the pointer moved relative to the window when a pass moved it.
        let mut shift = Vec2::ZERO;
        let mut again = false;
        for pass in 0..PASSES {
            let mut raw = self.input.take_egui_input(&self.window);
            if shift != Vec2::ZERO {
                // The cursor stays put on screen while the window moves under it;
                // without this the next pass sees it off the tooltip that grew
                // the window and shrinks it back.
                if let Some(p) = host.ctx.input(|i| i.pointer.latest_pos()) {
                    raw.events.push(egui::Event::PointerMoved(p + shift));
                }
            }
            let (w, h) = target.1;
            raw.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w as f32, h as f32) / ppp,
            ));
            let t = raw.time.unwrap_or(0.0);
            let state = host.shell.state.borrow().clone();
            let input = FrameInput {
                state: &state,
                prefs: &host.shell.prefs,
                now: Utc::now(),
                tz: Local::now().offset().fix(),
                t,
                reduced_motion: host.reduced_motion,
                offset_y: self.offset_y,
                focused: self.focused,
            };
            let mut shown = None;
            let output = host
                .ctx
                .run_pass(raw, |ctx| shown = Some(host.widget.show(ctx, &input)));
            let shown = shown.expect("the widget ran");
            self.input
                .handle_platform_output(&self.window, output.platform_output);

            // Rendered even when not shown: the renderer must take every texture delta.
            let primitives = host.ctx.tessellate(output.shapes, output.pixels_per_point);
            let (width, height) = (w.max(1) as usize, h.max(1) as usize);
            let pixels = self.surface.pixels(width, height);
            self.renderer.render(
                pixels,
                width,
                height,
                &primitives,
                output.textures_delta,
                output.pixels_per_point,
            );

            for action in shown.actions {
                apply(action, &self.window, &mut host.shell, &mut after);
            }
            if current.is_some() {
                let before = self.room.clone();
                let (next, offset_y) = self.fit(ppp, shown.size, shown.extra, target.0);
                let changed = next != target || offset_y != self.offset_y;
                if changed && pass + 1 < PASSES {
                    self.offset_y = offset_y;
                    let (from, to) = (target.0, next.0);
                    shift = egui::vec2((from.0 - to.0) as f32, (from.1 - to.1) as f32) / ppp;
                    target = next; // paint again at the new bounds
                    continue;
                }
                if changed {
                    // Out of passes: show what was painted, and keep the popup
                    // room on those bounds so our own move never reads as a drag.
                    self.room = before;
                    again = true;
                }
            }
            let moved = current.is_some_and(|c| c.0 != target.0);
            if current.is_some_and(|c| c != target) {
                self.reshaped_at = Some(Instant::now());
            }
            self.present(moved.then_some(target.0));
            // Placing the hidden window at startup is not a move.
            if let (Some(rect), Some(true)) = (self.rect(), self.window.is_visible()) {
                let expanded = self.room.expanded();
                self.saver
                    .observe((rect.x, rect.y), Instant::now(), false, expanded);
            }

            let egui_delay = output
                .viewport_output
                .get(&ViewportId::ROOT)
                .map_or(Duration::MAX, |v| v.repaint_delay);
            let widget_delay = match shown.repaint {
                Repaint::Idle => Duration::MAX,
                Repaint::After(d) => d,
                Repaint::Now => Duration::ZERO,
            };
            let widget_delay = if again { Duration::ZERO } else { widget_delay };
            self.next_repaint = Instant::now().checked_add(egui_delay.min(widget_delay));
            break;
        }
        after
    }

    /// Bounds for the widget (`size`) plus its popup room (`extra`), points, for
    /// a window at `pos`: grown downward, or upward when there is no room below.
    /// Returns the bounds and how far down the widget sits in them, points.
    fn fit(&mut self, ppp: f32, size: Vec2, extra: f32, pos: (i32, i32)) -> (Placement, f32) {
        let (base, extra) = fit_bounds(ppp, size, extra);
        let work = self.hwnd.and_then(platform::work_area_of).unwrap_or(Rect {
            x: pos.0,
            y: pos.1,
            w: base.0,
            h: base.1 + extra,
        });
        let b = self.room.apply(pos, base, extra, work);
        ((b.pos, b.size), b.offset_y as f32 / ppp)
    }

    /// Shows the pixels, moving the window to `pos` in the same step.
    fn present(&mut self, pos: Option<(i32, i32)>) {
        match self.surface.present(pos) {
            Ok(()) => self.present_error = None,
            Err(e) => {
                let msg = e.to_string();
                if self.present_error.as_ref() != Some(&msg) {
                    tracing::warn!("presenting the window failed: {msg}");
                    self.present_error = Some(msg);
                }
            }
        }
    }
}

fn apply(action: Action, window: &Window, shell: &mut Shell, after: &mut After) {
    let save = |settings: &SettingsStore, f: &dyn Fn(&mut limits_core::Settings)| {
        settings
            .update(f)
            .map(drop)
            .inspect_err(|e| tracing::warn!("cannot save settings: {e}"))
    };
    match action {
        Action::Send(cmd) => {
            if let Err(e) = shell.cmd.try_send(cmd) {
                tracing::warn!("cannot send a command to the engine: {e}");
            }
        }
        Action::SetLang(lang) => {
            let code = match lang {
                Lang::En => "en",
                Lang::Fr => "fr",
            };
            shell.prefs.lang = lang;
            let _ = save(&shell.settings, &|s| s.language = Some(code.to_owned()));
        }
        Action::SetAlwaysOnTop(on) => {
            window.set_window_level(level(on));
            shell.prefs.always_on_top = on;
            let _ = save(&shell.settings, &|s| s.always_on_top = on);
        }
        Action::SetStartWithWindows(on) => {
            if shell.manage_autostart {
                let synced = std::env::current_exe().and_then(|exe| {
                    platform::autostart::sync(
                        &platform::autostart::RunKey::user(),
                        crate::AUTOSTART_NAME,
                        on,
                        &exe,
                    )
                });
                if let Err(e) = synced {
                    tracing::warn!(on, "cannot change autostart: {e}");
                    return;
                }
            } else {
                tracing::info!(on, "dev run: autostart left untouched");
            }
            shell.prefs.start_with_windows = on;
            let _ = save(&shell.settings, &|s| s.start_with_windows = on);
        }
        Action::OpenUrl(url) => {
            if let Err(e) = platform::open_url(url) {
                tracing::warn!(url, "cannot open the link: {e}");
            }
        }
        Action::Minimize => window.set_minimized(true),
        // The widget closes its popups on a drag, so this frame's fit already put
        // the window back to its resting bounds; the drag starts once it is shown.
        Action::StartDrag => after.drag = true,
        Action::Quit => after.quit = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn fit_rounds_up_at_fractional_scale() {
        assert_eq!(fit_bounds(1.25, vec2(244.0, 145.6), 0.0), ((305, 182), 0));
        assert_eq!(fit_bounds(1.5, vec2(244.0, 146.0), 92.3), ((366, 219), 139));
    }

    #[test]
    fn move_saver_debounces_500ms() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut s = MoveSaver::default();
        s.observe((10, 10), ms(0), false, false);
        s.observe((20, 30), ms(300), false, false);
        assert_eq!(s.due(ms(700)), None);
        assert_eq!(s.due(ms(800)), Some((20, 30)));
        assert_eq!(s.due(ms(900)), None, "saved once");
    }

    #[test]
    fn move_saver_ignores_minimized_and_expanded() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut s = MoveSaver::default();
        s.observe((100, 100), ms(0), false, false);
        s.observe((-32000, -32000), ms(100), true, false);
        assert_eq!(s.due(ms(1000)), None, "minimizing is not a move");
        s.observe((100, 100), ms(1100), false, false);
        assert_eq!(s.due(ms(2000)), None, "restoring is not a move");
        // A drag, then the window grows upward for a popup: the rest is saved.
        s.observe((300, 300), ms(3000), false, false);
        s.observe((300, 208), ms(3100), false, true);
        assert_eq!(s.due(ms(3499)), None);
        assert_eq!(s.due(ms(3500)), Some((300, 300)));
    }

    #[test]
    fn move_saver_skips_a_round_trip() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut s = MoveSaver::default();
        s.observe((100, 100), ms(0), false, false);
        s.observe((100, 40), ms(10), false, false);
        s.observe((100, 100), ms(20), false, false);
        assert_eq!(s.due(ms(600)), None, "back where it was");
        assert_eq!(s.deadline(), None);
    }
}
