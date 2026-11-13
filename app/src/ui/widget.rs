//! The widget: glass box, rings view (connected) and the connect panels (`connect.rs`).
//!
//! Layout in CSS px = egui points (`ui/src/index.css` `.wd`): 1 px border; rings row padding
//! 24 top, 14 sides, 22 bottom, two 104 wide columns 6 apart → box 244 wide.

#[cfg(test)]
use std::collections::BTreeMap;

use std::time::Duration;

use egui::{pos2, vec2, Color32, CursorIcon, Id, LayerId, Order, Painter, Rect, Vec2};
use limits_core::{Conn, LimitKey, RingView};

use super::connect::{Panel, PANEL_W};
use super::controls::{ease_css, Controls};
use super::input::PressTracker;
use super::menu_view::Menu;
use super::resets::Resets;
use super::ring::{err_line, is_near_limit};
#[cfg(test)]
use super::rings::Live;
use super::rings::{self, Column, RingTweens, COLUMN_H, COLUMN_W, LINE_HEIGHT};
use super::room::{self, RingRooms};
use super::tooltip::{self, Tip};
use super::{colors, draw, fonts, pill, sparks, Action, FrameInput, FrameOutput, Repaint};
#[cfg(test)]
use crate::anim::RingFrame;

/// How often a shown countdown (error retry) or an open tooltip ("updated X ago") ticks.
const TICK: Duration = Duration::from_secs(1);

/// When the host must paint again without input: at once while something moves (animations,
/// transitions, the Waiting spinner, a popup's first frame), each second while a countdown or
/// a tooltip shows, else only on input or state change.
pub fn repaint_policy(
    animating: bool,
    spinner: bool,
    countdown: bool,
    tooltip_open: bool,
) -> Repaint {
    if animating || spinner {
        Repaint::Now
    } else if countdown || tooltip_open {
        Repaint::After(TICK)
    } else {
        Repaint::Idle
    }
}

const BORDER: f32 = 1.0;
const ROW_PAD_TOP: f32 = 24.0;
const ROW_PAD_SIDE: f32 = 14.0;
const ROW_PAD_BOTTOM: f32 = 22.0;
const ROW_GAP: f32 = 6.0;
/// `.wd .foot { padding: 0 12px 9px; font-size: 10.5px }`.
const FOOT_SIZE: f32 = 10.5;
const FOOT_PAD_X: f32 = 12.0;
const FOOT_PAD_BOTTOM: f32 = 9.0;
/// The rings box: border, padded row of two columns, and the error foot line.
fn rings_size(error: bool) -> Vec2 {
    let w = 2.0 * BORDER + 2.0 * ROW_PAD_SIDE + 2.0 * COLUMN_W + ROW_GAP;
    let mut h = 2.0 * BORDER + ROW_PAD_TOP + COLUMN_H + ROW_PAD_BOTTOM;
    if error {
        h += FOOT_SIZE * LINE_HEIGHT + FOOT_PAD_BOTTOM;
    }
    vec2(w, h)
}

/// CSS `filter: grayscale(1); opacity: .5` on the rings row, per color (luma, half alpha).
fn gray_half(c: Color32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let l = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    let l = l.round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(l, l, l, (f32::from(a) * 0.5).round() as u8)
}

/// The two ring columns of the rings box `rect` (also the tooltips' hit boxes).
fn column_rects(rect: Rect) -> [Rect; 2] {
    let row_top = rect.top() + BORDER + ROW_PAD_TOP;
    [0, 1].map(|i| {
        let x = rect.left() + BORDER + ROW_PAD_SIDE + i as f32 * (COLUMN_W + ROW_GAP);
        rings::column_rect(pos2(x, row_top))
    })
}

const TIP_NAMES: [&str; 2] = ["tip_session", "tip_weekly"];

/// The ring's center below its column top (the 64 px SVG box's middle).
const RING_C: f32 = 32.0;

/// Reset state is indexed by limit: session 0, weekly 1.
fn key_index(key: LimitKey) -> usize {
    match key {
        LimitKey::Session => 0,
        LimitKey::Weekly => 1,
    }
}

pub struct Widget {
    rings: [RingTweens; 2],
    resets: Resets,
    /// The ring views drawn on the last frame (held ones while locked), `None` without rings.
    shown: Option<[RingView; 2]>,
    /// The rings were on screen last frame (else the next rings view plays the entrance).
    rings_up: bool,
    #[cfg(test)]
    debug_frames: [Option<RingFrame>; 2],
    #[cfg(test)]
    debug_t: f64,
    controls: Controls,
    presses: PressTracker,
    menu: Menu,
    #[cfg(test)]
    debug_rects: BTreeMap<&'static str, Rect>,
    #[cfg(test)]
    debug_menu_items: Vec<(String, Rect)>,
}

impl Default for Widget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget {
    pub fn new() -> Widget {
        Widget {
            rings: [RingTweens::new(), RingTweens::new()],
            resets: Resets::default(),
            shown: None,
            rings_up: false,
            #[cfg(test)]
            debug_frames: [None, None],
            #[cfg(test)]
            debug_t: 0.0,
            controls: Controls::default(),
            presses: PressTracker::default(),
            menu: Menu::default(),
            #[cfg(test)]
            debug_rects: BTreeMap::new(),
            #[cfg(test)]
            debug_menu_items: vec![],
        }
    }

    /// Rects of the clickable parts drawn by the last `show`, by name.
    #[cfg(test)]
    pub fn debug_rects(&self) -> &BTreeMap<&'static str, Rect> {
        &self.debug_rects
    }

    /// Labels and rects of the open menu's items, as last shown.
    #[cfg(test)]
    pub fn debug_menu_items(&self) -> &[(String, Rect)] {
        &self.debug_menu_items
    }

    /// Acts as if the rings had been on screen already: the first rings view plays no entrance.
    #[cfg(test)]
    pub fn skip_entrance(&mut self) {
        self.rings_up = true;
    }

    #[cfg(test)]
    pub fn debug_resets(&self) -> &Resets {
        &self.resets
    }

    /// The ring views drawn by the last `show`.
    #[cfg(test)]
    pub fn debug_views(&self) -> Option<&[RingView; 2]> {
        self.shown.as_ref()
    }

    /// The reset frames drawn by the last `show`.
    #[cfg(test)]
    pub fn debug_frames(&self) -> &[Option<RingFrame>; 2] {
        &self.debug_frames
    }

    /// The rings' live (transitioned) values at the last `show`'s clock.
    #[cfg(test)]
    pub fn debug_live(&self) -> [Live; 2] {
        let t = self.debug_t;
        [self.rings[0].value(t), self.rings[1].value(t)]
    }

    #[cfg_attr(not(test), expect(unused_variables))]
    fn note(&mut self, name: &'static str, rect: Rect) {
        #[cfg(test)]
        self.debug_rects.insert(name, rect);
    }

    /// Paints one frame and reports what the host must do.
    pub fn show(&mut self, ctx: &egui::Context, input: &FrameInput) -> FrameOutput {
        #[cfg(test)]
        {
            self.debug_rects.clear();
            self.debug_menu_items.clear();
        }
        let mut clicks = self.presses.frame(ctx);
        let state = input.state;
        let rings = match (&state.conn, &state.rings) {
            (Conn::Connected, Some(rings)) => Some(rings),
            _ => None,
        };
        let error = state.error.is_some();
        let panel = match rings {
            Some(_) => None,
            None => Some(Panel::layout(
                &ctx.layer_painter(LayerId::background()),
                input,
            )),
        };
        let size = match &panel {
            None => rings_size(error),
            Some(panel) => vec2(PANEL_W + 2.0 * BORDER, panel.height),
        };
        let rect = Rect::from_min_size(pos2(0.0, input.offset_y), size);
        let painter = ctx
            .layer_painter(LayerId::background())
            .with_clip_rect(rect);
        let warn = !error && is_near_limit(rings);
        draw::glass(&painter, rect, warn);
        // Popups paint above the widget, anywhere in the window (which is size + extra tall).
        let window = ctx.content_rect();
        let popups = ctx
            .layer_painter(LayerId::new(
                Order::Foreground,
                Id::unique("limitrings.popups"),
            ))
            .with_clip_rect(window);

        #[cfg(test)]
        {
            self.debug_t = input.t;
        }
        let mut animating = false;
        let mut spinner = false;
        let mut actions = vec![];
        let mut nodrag = vec![];
        // The menu first: it sits on top, so it takes the clicks on its box.
        let menu = self.menu.update(
            ctx,
            &popups,
            rect,
            input,
            &mut self.presses,
            &mut clicks,
            &mut actions,
        );
        match (rings, &panel) {
            (Some(rings), _) => {
                animating = self.paint_rings(&painter, rect, rings, input);
            }
            (None, panel) => {
                self.rings.iter_mut().for_each(RingTweens::reset);
                // Rings gone: no lock, glow, sparks or pill left for when they come back.
                self.resets.clear(state.reset_seq);
                self.shown = None;
                self.rings_up = false;
                if let Some(panel) = panel {
                    let mut noted = vec![];
                    let done = panel.paint(ctx, &painter, rect, input, &clicks, &mut |n, r| {
                        noted.push((n, r));
                    });
                    for (n, r) in noted {
                        self.note(n, r);
                    }
                    actions.extend(done.actions);
                    spinner = done.animating;
                    nodrag = done.nodrag;
                }
            }
        }

        if ctx.pointer_hover_pos().is_some_and(|q| rect.contains(q)) {
            ctx.set_cursor_icon(CursorIcon::Grab);
        }
        let mut noted = vec![];
        let ctl = self.controls.show(
            ctx,
            &painter,
            rect,
            input,
            rings.is_some(),
            &clicks,
            &mut |n, r| {
                noted.push((n, r));
            },
        );
        for (n, r) in noted {
            self.note(n, r);
        }
        actions.extend(ctl.actions);
        animating |= ctl.animating;
        nodrag.extend(ctl.nodrag);
        // Reset effects paint over the rings and the controls (`.glow`, `.chip`, `.sparks`).
        if rings.is_some() {
            self.paint_effects(&painter, rect, input);
        }
        // A primary press on the glass, off every control and popup, moves the window.
        let drag = clicks.press.is_some_and(|q| {
            rect.contains(q) && !nodrag.iter().any(|r| r.contains(q)) && !clicks.is_blocked(q)
        });
        if drag {
            actions.push(Action::StartDrag);
            // The OS drag loop may eat the release: forget the press now.
            self.presses.swallow(&mut clicks);
        }

        // Tooltips: never while the menu is open or the button is down (a drag). A drag
        // closes everything at once so the host settles the window first.
        let menu = if drag {
            self.menu.close();
            None
        } else {
            menu
        };
        let blocked = menu.is_some() || self.presses.is_down() || drag;
        let pointer = ctx.pointer_hover_pos();
        let mut rooms = RingRooms::default();
        let mut tips = vec![];
        let mut tip_open = false;
        for (i, hit) in column_rects(rect).into_iter().enumerate() {
            let id = Id::unique(("limitrings.tip", i));
            let view = rings.map(|r| &r[i]);
            let hover = !blocked && view.is_some() && pointer.is_some_and(|q| hit.contains(q));
            let alpha = if blocked || view.is_none() {
                ctx.animate_bool_with_time(id, false, 0.0)
            } else {
                ctx.animate_bool_with_time_and_easing(id, hover, tooltip::FADE, ease_css)
            };
            let (Some(view), true) = (view, hover || alpha > 0.0) else {
                continue;
            };
            animating |= alpha != if hover { 1.0 } else { 0.0 };
            tip_open |= hover;
            let tip = Tip::layout(&popups, view, input, size.x - 2.0 * room::PAD);
            let room = room::tip_room(hit.bottom() - rect.top(), tip.size.y, size.y);
            rooms = room::with_ring_room(rooms, view.key, room);
            let min = tooltip::place(hit, tip.size, window);
            self.note(TIP_NAMES[i], Rect::from_min_size(min, tip.size));
            tips.push((tip, min, alpha));
        }
        let extra = room::max_room(&rooms).max(menu.as_ref().map_or(0.0, |m| m.room));

        // The first frame of a popup may come before the window grew: draw it invisible.
        // The room is below the widget, or above it when the host grew the window upward.
        let fits = window.height() + 0.5 >= size.y + extra;
        for (tip, min, alpha) in tips {
            let mut p = popups.clone();
            p.multiply_opacity(if fits { alpha } else { 0.0 });
            tip.paint(&p, min);
        }
        if let Some(m) = &menu {
            self.note("menu", m.rect());
            #[cfg(test)]
            {
                self.debug_menu_items = m.items();
            }
            let mut p = popups.clone();
            p.multiply_opacity(if fits { 1.0 } else { 0.0 });
            m.layout.paint(ctx, &p, m.min, pointer);
        }
        // The error retry countdown shows on the rings' foot line or as the lone panel line.
        let countdown = state.conn == Conn::Connected && error;
        let repaint = repaint_policy(animating || !fits, spinner, countdown, tip_open);
        FrameOutput {
            actions,
            size,
            extra,
            repaint,
        }
    }

    /// The rings row (and the error foot line), with the reset animations of `self.resets`.
    /// Returns whether a transition or animation is still running.
    fn paint_rings(
        &mut self,
        p: &Painter,
        rect: Rect,
        rings: &[RingView; 2],
        input: &FrameInput,
    ) -> bool {
        let lang = input.prefs.lang;
        let t = input.t;
        let error = input.state.error.as_ref();
        let fx: &dyn Fn(Color32) -> Color32 = if error.is_some() { &gray_half } else { &|c| c };
        let row_top = rect.top() + BORDER + ROW_PAD_TOP;
        let cols = column_rects(rect);
        let centers = cols.map(|c| pos2(c.center().x, c.top() + RING_C) - rect.min.to_vec2());
        self.resets.observe(
            input.state,
            self.shown.as_ref(),
            rings,
            centers,
            t,
            input.reduced_motion,
            lang,
        );
        for (i, from) in self.resets.advance(t, input.reduced_motion) {
            self.rings[i].hand_over(from);
        }
        let entering = !self.rings_up && !input.reduced_motion;
        self.rings_up = true;
        let mut animating = self.resets.animating();
        let mut shown = rings.clone();
        for (i, view) in rings.iter().enumerate() {
            let k = key_index(view.key);
            let held = self.resets.held(k).cloned().unwrap_or_else(|| view.clone());
            let tweens = &mut self.rings[i];
            if entering {
                tweens.enter(&held, t, i);
            } else {
                tweens.update(&held, t);
            }
            animating |= tweens.animating(t);
            let frame = self.resets.frame(k, t);
            let col = Column {
                view: &held,
                live: tweens.value(t),
                frame: frame.as_ref(),
                ok: self.resets.ok(k, t),
                lang,
                now: input.now,
                tz: input.tz,
            };
            rings::paint_column(p, cols[i], &col, fx);
            let name = match view.key {
                LimitKey::Session => "ring_session",
                LimitKey::Weekly => "ring_weekly",
            };
            self.note(name, cols[i]);
            #[cfg(test)]
            {
                self.debug_frames[i] = frame;
            }
            shown[i] = held;
        }
        self.shown = Some(shown);
        if let Some(err) = error {
            let top = row_top + COLUMN_H + ROW_PAD_BOTTOM;
            let text = err_line(err.retry_at, input.now, lang);
            let font = fonts::regular(FOOT_SIZE);
            let max_width = rect.width() - 2.0 * (BORDER + FOOT_PAD_X);
            let cx = rect.center().x;
            let shown =
                rings::centered_line_fit(p, &text, font, colors::ERR_TEXT, cx, top, max_width);
            self.note("err_foot", shown);
        }
        animating
    }

    /// The surprise glow, the pill and the sparks over the widget box `rect`.
    fn paint_effects(&mut self, p: &Painter, rect: Rect, input: &FrameInput) {
        let t = input.t;
        sparks::paint_glow(p, rect, self.resets.glow_alpha(t));
        if let Some((spec, frame)) = self.resets.pill(t) {
            let r = pill::paint(p, rect, spec, &frame);
            self.note("pill", r);
        }
        sparks::paint_sparks(p, rect, &self.resets.sparks(t));
    }
}

#[cfg(test)]
pub(crate) mod snapshot_tests {
    use chrono::{DateTime, TimeZone, Utc};
    use egui::{pos2, vec2, Color32, LayerId, Rect};
    use egui_kittest::Harness;
    use limits_core::{AppState, Command};

    use super::*;
    use crate::ui::Action;
    use crate::ui::{fixtures, fonts, Lang, Prefs};

    /// Animation clock far past every tween and entrance.
    const T_SETTLED: f64 = 1000.0;
    /// Stands in for Phase 1's dark mock desktop behind the glass.
    const BACKDROP: Color32 = Color32::from_rgb(17, 17, 24);

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 3, 10, 30, 0).unwrap()
    }

    fn prefs(lang: Lang) -> Prefs {
        Prefs {
            lang,
            always_on_top: true,
            start_with_windows: false,
        }
    }

    fn input(f: &Fixture) -> FrameInput<'_> {
        FrameInput {
            state: &f.state,
            prefs: &f.prefs,
            now: now(),
            tz: fixtures::tz(),
            t: f.t,
            reduced_motion: f.reduced,
            offset_y: f.offset_y,
            focused: f.focused,
        }
    }

    pub(crate) struct Fixture {
        pub widget: Widget,
        pub state: AppState,
        pub prefs: Prefs,
        pub out: Option<FrameOutput>,
        /// Animation clock of the next frame.
        pub t: f64,
        /// The window has focus.
        pub focused: bool,
        /// Points the widget sits below the window top (window grown upward).
        pub offset_y: f32,
        /// Windows "Show animations" is off.
        pub reduced: bool,
    }

    /// Paints the backdrop and the widget, settled, into a pass of `ctx`.
    pub(crate) fn frame(ctx: &egui::Context, f: &mut Fixture) {
        ctx.layer_painter(LayerId::background())
            .rect_filled(ctx.content_rect(), 0, BACKDROP);
        let mut widget = std::mem::take(&mut f.widget);
        f.out = Some(widget.show(ctx, &input(f)));
        f.widget = widget;
    }

    /// The widget showing `kind` as if it had been on screen for a while (no entrance).
    pub(crate) fn fixture(kind: &str, lang: Lang) -> Fixture {
        let mut f = fixture_live(kind, lang);
        f.widget.skip_entrance();
        f
    }

    /// The widget showing `kind` for the first time: the rings play their entrance.
    pub(crate) fn fixture_live(kind: &str, lang: Lang) -> Fixture {
        Fixture {
            widget: Widget::new(),
            state: fixtures::scenario(kind, now()),
            prefs: prefs(lang),
            out: None,
            t: T_SETTLED,
            focused: true,
            offset_y: 0.0,
            reduced: false,
        }
    }

    pub(crate) type H = Harness<'static, Fixture>;

    /// Plays the host: grows the harness to `size + extra` until the popups fit, settled.
    pub(crate) fn fit(h: &mut H) {
        for _ in 0..4 {
            let out = h.state().out.as_ref().expect("shown");
            let want = vec2(out.size.x, out.size.y + out.extra);
            if (h.ctx.content_rect().size() - want).length() < 0.01 {
                break;
            }
            h.set_size(want);
            h.run();
        }
    }

    /// Moves the pointer to `at`, settles, and grows the window for any popup.
    pub(crate) fn hover(h: &mut H, at: egui::Pos2) {
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.run();
        fit(h);
    }

    /// Moves the pointer to `at` and presses and releases `button` there, in one frame.
    pub(crate) fn click_at(h: &mut H, at: egui::Pos2, button: egui::PointerButton) {
        let input = h.input_mut();
        input.events.push(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.step();
    }

    pub(crate) fn render(kind: &str, lang: Lang, ppp: f32) -> Harness<'static, Fixture> {
        render_dt(kind, lang, ppp, 0.25)
    }

    /// [`render`] with egui's clock advancing `step_dt` seconds per harness step.
    pub(crate) fn render_dt(
        kind: &str,
        lang: Lang,
        ppp: f32,
        step_dt: f32,
    ) -> Harness<'static, Fixture> {
        let mut h = Harness::builder()
            .with_size(vec2(244.0, 146.0))
            .with_pixels_per_point(ppp)
            .with_step_dt(step_dt)
            .with_render_cursor(false)
            .with_context_setup(fonts::install)
            .build_ui_state(
                |ui, f: &mut Fixture| frame(ui.ctx(), f),
                fixture(kind, lang),
            );
        let size = h.state().out.as_ref().expect("shown").size;
        h.set_size(size);
        h.run();
        h
    }

    /// One plain pass (no renderer): the widget's output for `kind`, English.
    pub(crate) fn show(kind: &str) -> FrameOutput {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        let mut f = fixture(kind, Lang::En);
        let mut out = ctx.run_pass(egui::RawInput::default(), |ctx| frame(ctx, &mut f));
        out.textures_delta.clear();
        f.out.expect("shown")
    }

    fn show_size(kind: &str) -> egui::Vec2 {
        show(kind).size
    }

    #[test]
    fn rings_box_is_244_wide() {
        assert_eq!(show_size("connected").x, 244.0);
    }

    #[test]
    fn error_adds_the_foot_line() {
        assert!(show_size("error").y > show_size("connected").y + 15.0);
    }

    #[test]
    fn no_actions_when_idle() {
        assert!(show("connected").actions.is_empty());
    }

    #[test]
    fn rings_box_height_comes_from_the_layout() {
        // 1 + 24 + 64 + 6 + 11×1.2 + 2 + 10.5×1.2 + 22 + 1 (CSS line height 1.2).
        assert!((show_size("connected").y - 145.8).abs() < 0.01);
        // The foot line: 10.5×1.2 + 9 bottom padding.
        assert!((show_size("error").y - 167.4).abs() < 0.01);
    }

    #[test]
    fn other_states_are_connect_panel_wide() {
        assert_eq!(show_size("disconnected").x, 244.0);
        assert_eq!(show("waiting").extra, 0.0);
    }

    #[test]
    fn rings_columns_are_noted_for_click_tests() {
        let h = render("connected", Lang::En, 1.0);
        let rects = h.state().widget.debug_rects();
        let session = rects["ring_session"];
        let weekly = rects["ring_weekly"];
        assert_eq!(session.min, pos2(15.0, 25.0));
        assert_eq!(weekly.min, pos2(125.0, 25.0));
        assert_eq!(session.width(), 104.0);
    }

    /// `connected_en` through the window's CPU rasterizer (`render.rs`) matches the wgpu snapshot.
    #[test]
    fn connected_soft_render_matches_snapshot() {
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        let (w, h) = (244u32, 146u32);
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(244.0, 145.8))),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(1.0);
        let mut f = fixture("connected", Lang::En);
        let out = ctx.run_pass(input, |ctx| frame(ctx, &mut f));
        let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut canvas = vec![0u32; (w * h) as usize];
        crate::render::SoftRenderer::default().render(
            &mut canvas,
            w as usize,
            h as usize,
            &primitives,
            out.textures_delta,
            out.pixels_per_point,
        );
        let soft = image::RgbaImage::from_fn(w, h, |x, y| {
            let [b, g, r, a] = canvas[(y * w + x) as usize].to_le_bytes();
            image::Rgba([r, g, b, a])
        });
        if let Some(dir) = std::env::var_os("LIMITRINGS_DUMP_DIR") {
            soft.save(std::path::Path::new(&dir).join("connected_en_soft.png"))
                .unwrap();
        }
        let snapshot = image::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/connected_en.png"
        ))
        .unwrap()
        .to_rgba8();
        assert_eq!(soft.dimensions(), snapshot.dimensions());
        let (mut worst, mut off) = (0u8, 0usize);
        for (a, b) in soft.pixels().zip(snapshot.pixels()) {
            let d = (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap();
            worst = worst.max(d);
            off += usize::from(d > 8);
        }
        eprintln!("soft vs wgpu: worst channel diff {worst}, {off} pixels off by > 8");
        assert!(
            off <= 50,
            "{off} pixels differ by more than 8 (worst {worst})"
        );
    }

    #[test]
    fn error_foot_fits_inside_its_padding() {
        // 244 − 2 × (1 + 12).
        for lang in [Lang::En, Lang::Fr] {
            let h = render("error", lang, 1.0);
            let foot = h.state().widget.debug_rects()["err_foot"];
            assert!(
                foot.left() >= 13.0 && foot.right() <= 231.0,
                "{lang:?} {foot:?}"
            );
        }
        // A line far too long is shrunk to the same width.
        let ctx = egui::Context::default();
        fonts::install(&ctx);
        let mut out = ctx.run_pass(egui::RawInput::default(), |ctx| {
            let p = ctx.layer_painter(LayerId::background());
            let long = "claude.ai not responding · retry in 2 min · ".repeat(3);
            let font = fonts::regular(10.5);
            let r = rings::centered_line_fit(&p, &long, font, Color32::WHITE, 122.0, 0.0, 218.0);
            assert!(r.width() <= 218.0 && r.width() > 200.0, "{r:?}");
            assert!((r.center().x - 122.0).abs() < 0.01);
        });
        out.textures_delta.clear();
    }

    fn snap(kind: &str, lang: Lang, ppp: f32, name: &str) {
        render(kind, lang, ppp).snapshot(name);
    }

    #[test]
    fn connected_en() {
        snap("connected", Lang::En, 1.0, "connected_en");
    }
    #[test]
    fn connected_fr() {
        snap("connected", Lang::Fr, 1.0, "connected_fr");
    }
    #[test]
    fn connected_ontrack_en() {
        snap("ontrack", Lang::En, 1.0, "connected_ontrack_en");
    }
    #[test]
    fn connected_ontrack_fr() {
        snap("ontrack", Lang::Fr, 1.0, "connected_ontrack_fr");
    }
    #[test]
    fn idle_en() {
        snap("idle", Lang::En, 1.0, "idle_en");
    }
    #[test]
    fn idle_fr() {
        snap("idle", Lang::Fr, 1.0, "idle_fr");
    }
    #[test]
    fn ultra_en() {
        snap("ultra", Lang::En, 1.0, "ultra_en");
    }
    #[test]
    fn ultra_fr() {
        snap("ultra", Lang::Fr, 1.0, "ultra_fr");
    }
    #[test]
    fn near_en() {
        snap("near", Lang::En, 1.0, "near_en");
    }
    #[test]
    fn near_fr() {
        snap("near", Lang::Fr, 1.0, "near_fr");
    }
    #[test]
    fn error_en() {
        snap("error", Lang::En, 1.0, "error_en");
    }
    #[test]
    fn error_fr() {
        snap("error", Lang::Fr, 1.0, "error_fr");
    }
    #[test]
    fn connected_ontrack_en_125() {
        snap("ontrack", Lang::En, 1.25, "connected_ontrack_en@1.25x");
    }
    #[test]
    fn connected_ontrack_en_150() {
        snap("ontrack", Lang::En, 1.5, "connected_ontrack_en@1.5x");
    }

    /// Clicks the center of the named rect; returns the actions of the next frame.
    fn click(kind: &str, name: &str) -> Vec<crate::ui::Action> {
        let mut h = render(kind, Lang::En, 1.0);
        let at = h.state().widget.debug_rects()[name].center();
        let input = h.input_mut();
        input.events.push(egui::Event::PointerMoved(at));
        for pressed in [true, false] {
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.step();
        h.state().out.as_ref().expect("shown").actions.clone()
    }

    #[test]
    fn connect_button_sends_connect() {
        assert_eq!(
            click("disconnected", "connect_button"),
            vec![Action::Send(Command::Connect)]
        );
        assert_eq!(
            click("expired", "connect_button"),
            vec![Action::Send(Command::Connect)]
        );
    }

    #[test]
    fn waiting_links() {
        assert_eq!(
            click("waiting", "link_0"),
            vec![Action::Send(Command::Connect)]
        );
        assert_eq!(
            click("waiting", "link_1"),
            vec![Action::Send(Command::CancelConnect)]
        );
    }

    #[test]
    fn get_firefox_opens_download_page() {
        assert_eq!(
            click("needsFirefox", "connect_button"),
            vec![Action::OpenUrl("https://www.mozilla.org/firefox/")]
        );
    }

    #[test]
    fn waiting_repaints_continuously_others_idle() {
        assert_eq!(show("waiting").repaint, Repaint::Now);
        assert_eq!(show("disconnected").repaint, Repaint::Idle);
    }

    #[test]
    fn panel_text_wraps_inside_the_content_width() {
        for kind in [
            "disconnected",
            "timedout",
            "waiting",
            "needsFirefox",
            "expired",
        ] {
            for lang in [Lang::En, Lang::Fr] {
                let h = render(kind, lang, 1.0);
                let body = h.state().widget.debug_rects()["body"];
                assert!(
                    body.left() >= 19.0 && body.right() <= 225.01,
                    "{kind} {body:?}"
                );
            }
        }
    }

    fn snap_panel(kind: &str, lang: Lang, name: &str) {
        render(kind, lang, 1.0).snapshot(name);
    }
    #[test]
    fn disconnected_en() {
        snap_panel("disconnected", Lang::En, "disconnected_en");
    }
    #[test]
    fn disconnected_fr() {
        snap_panel("disconnected", Lang::Fr, "disconnected_fr");
    }
    #[test]
    fn timedout_en() {
        snap_panel("timedout", Lang::En, "timedout_en");
    }
    #[test]
    fn waiting_en() {
        snap_panel("waiting", Lang::En, "waiting_en");
    }
    #[test]
    fn waiting_fr() {
        snap_panel("waiting", Lang::Fr, "waiting_fr");
    }
    #[test]
    fn needs_firefox_en() {
        snap_panel("needsFirefox", Lang::En, "needs_firefox_en");
    }
    #[test]
    fn needs_firefox_fr() {
        snap_panel("needsFirefox", Lang::Fr, "needs_firefox_fr");
    }
    #[test]
    fn expired_en() {
        snap_panel("expired", Lang::En, "expired_en");
    }
    #[test]
    fn expired_fr() {
        snap_panel("expired", Lang::Fr, "expired_fr");
    }
    #[test]
    fn connected_no_rings_error_en() {
        let mut f = fixture("error", Lang::En);
        f.state.rings = None;
        let mut h = Harness::builder()
            .with_size(vec2(244.0, 146.0))
            .with_context_setup(fonts::install)
            .build_ui_state(|ui, f: &mut Fixture| frame(ui.ctx(), f), f);
        let size = h.state().out.as_ref().expect("shown").size;
        h.set_size(size);
        h.run();
        h.snapshot("connected_no_rings_error_en");
    }

    // ---- reset animations, entrance and repaint policy (Task 13) ----

    use crate::anim::{
        surprise_frame, PLANNED_END, PLANNED_PILL_AT, SURPRISE_END, SURPRISE_PILL_AT,
    };
    use crate::ui::resets::Resets;

    /// A widget driven pass by pass on a plain context (no renderer).
    pub(crate) struct Run {
        ctx: egui::Context,
        pub f: Fixture,
    }

    impl Run {
        pub(crate) fn new(f: Fixture) -> Run {
            let ctx = egui::Context::default();
            fonts::install(&ctx);
            let mut r = Run { ctx, f };
            r.frame();
            r
        }

        /// One pass at the fixture's current clock.
        pub(crate) fn frame(&mut self) -> &FrameOutput {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(244.0, 400.0))),
                ..Default::default()
            };
            let f = &mut self.f;
            let mut out = self.ctx.run_pass(raw, |ctx| frame(ctx, f));
            out.textures_delta.clear();
            self.f.out.as_ref().expect("shown")
        }

        /// One pass at animation clock `t`.
        pub(crate) fn at(&mut self, t: f64) -> &FrameOutput {
            self.f.t = t;
            self.frame()
        }

        /// Frames every 50 ms from the current clock to `t`, then one at `t`.
        pub(crate) fn play_to(&mut self, t: f64) {
            while self.f.t + 0.05 < t {
                let next = self.f.t + 0.05;
                self.at(next);
            }
            self.at(t);
        }

        /// Applies reset `kind` (sequence `seq`) to the current state and shows a frame.
        pub(crate) fn reset(&mut self, kind: &str, seq: u64) {
            self.f.state = fixtures::reset_state(&self.f.state, kind, seq, now());
            self.frame();
        }

        pub(crate) fn resets(&self) -> &Resets {
            self.f.widget.debug_resets()
        }
    }

    /// A settled `connected` widget, then reset `kind` at `T_SETTLED`.
    fn reset_run_at(kind: &str, reduced: bool) -> Run {
        let mut f = fixture("connected", Lang::En);
        f.reduced = reduced;
        let mut r = Run::new(f);
        r.reset(kind, 1);
        r
    }

    /// Reset `kind` played to its end, frame by frame.
    fn reset_run(kind: &str) -> Run {
        let mut r = reset_run_at(kind, false);
        r.play_to(T_SETTLED + 5.0);
        r
    }

    fn pill_shows(r: &Run) -> usize {
        r.resets().pill_shows()
    }

    #[test]
    fn policy() {
        assert_eq!(repaint_policy(false, false, false, false), Repaint::Idle);
        assert_eq!(
            repaint_policy(false, false, true, false),
            Repaint::After(Duration::from_secs(1))
        );
        assert_eq!(
            repaint_policy(false, false, false, true),
            Repaint::After(Duration::from_secs(1))
        );
        assert_eq!(repaint_policy(true, false, true, false), Repaint::Now);
        assert_eq!(repaint_policy(false, true, true, true), Repaint::Now);
    }

    #[test]
    fn countdowns_repaint_every_second() {
        assert_eq!(
            show("error").repaint,
            Repaint::After(Duration::from_secs(1))
        );
        let mut f = fixture("error", Lang::En);
        f.state.rings = None;
        let mut r = Run::new(f);
        assert_eq!(r.frame().repaint, Repaint::After(Duration::from_secs(1)));
        assert_eq!(show("connected").repaint, Repaint::Idle);
    }

    #[test]
    fn a_poll_with_unchanged_usage_does_not_animate() {
        // Regression (idle CPU): every 30 s poll recomputes the expected pace, moving the
        // tick by under a degree; that must not keep the host painting.
        let mut r = Run::new(fixture("connected", Lang::En));
        assert_eq!(r.at(T_SETTLED + 1.0).repaint, Repaint::Idle);
        let rings = r.f.state.rings.as_mut().expect("rings");
        for v in rings.iter_mut() {
            v.expected = v.expected.map(|e| e + 0.2);
        }
        assert_eq!(r.at(T_SETTLED + 31.0).repaint, Repaint::Idle);
    }

    #[test]
    fn settles_to_no_repaint() {
        let mut r = Run::new(fixture_live("connected", Lang::En));
        assert_eq!(r.at(T_SETTLED + 0.4).repaint, Repaint::Now);
        r.play_to(T_SETTLED + 2.0);
        assert_eq!(r.frame().repaint, Repaint::Idle);
    }

    #[test]
    fn entrance_grows_the_arcs_and_turns_the_tick_in() {
        let mut r = Run::new(fixture_live("connected", Lang::En));
        r.at(T_SETTLED + 0.4);
        let live = r.f.widget.debug_live();
        assert!(
            live[0].arc_fraction > 0.05 && live[0].arc_fraction < 0.62,
            "{live:?}"
        );
        assert!(live[0].tick_deg > 0.0 && live[0].tick_deg < 48.0 * 3.6);
        r.at(T_SETTLED + 2.0);
        let live = r.f.widget.debug_live();
        assert_eq!((live[0].arc_fraction, live[1].arc_fraction), (0.62, 0.34));
        // Reduced motion: no entrance.
        let mut f = fixture_live("connected", Lang::En);
        f.reduced = true;
        let r = Run::new(f);
        assert_eq!(r.f.widget.debug_live()[0].arc_fraction, 0.62);
        // A reconnect plays it again.
        let mut r = Run::new(fixture("connected", Lang::En));
        let rings = r.f.state.clone();
        r.f.state = fixtures::scenario("disconnected", now());
        r.frame();
        r.f.state = rings;
        r.at(T_SETTLED + 0.1);
        assert!(r.f.widget.debug_live()[0].arc_fraction < 0.1);
    }

    #[test]
    fn held_view_until_release() {
        let mut r = Run::new(fixture("connected", Lang::En));
        let old = r.f.state.rings.as_ref().unwrap()[0].resets_at;
        r.reset("planned", 1);
        let new = r.f.state.rings.as_ref().unwrap()[0].resets_at;
        assert_ne!(old, new);
        r.play_to(T_SETTLED + 1.0);
        assert_eq!(r.f.widget.debug_views().unwrap()[0].resets_at, old);
        assert_eq!(r.resets().locks(), [Some(1), None]);
        r.play_to(T_SETTLED + 2.2);
        assert_eq!(r.f.widget.debug_views().unwrap()[0].resets_at, new);
        assert_eq!(r.resets().locks(), [None, None]);
        // Handed over from empty and green: the arc and colors tween to the live values.
        let live = r.f.widget.debug_live()[0];
        assert!(live.arc_fraction < 0.01, "{live:?}");
        assert_ne!(live.num_color, colors::BLUE);
    }

    #[test]
    fn pill_only_once_per_batch() {
        assert_eq!(pill_shows(&reset_run("both")), 1);
        assert_eq!(pill_shows(&reset_run("bothPlanned")), 1);
        assert_eq!(pill_shows(&reset_run("planned")), 1);
    }

    #[test]
    fn pill_waits_for_its_moment() {
        let mut r = reset_run_at("planned", false);
        r.play_to(T_SETTLED + (PLANNED_PILL_AT - 10.0) / 1000.0);
        assert_eq!(pill_shows(&r), 0);
        r.play_to(T_SETTLED + (PLANNED_PILL_AT + 10.0) / 1000.0);
        assert_eq!(pill_shows(&r), 1);
        let mut r = reset_run_at("surprise", false);
        r.play_to(T_SETTLED + (SURPRISE_PILL_AT + 10.0) / 1000.0);
        assert_eq!(pill_shows(&r), 1);
    }

    #[test]
    fn newer_batch_takes_over_ring() {
        let mut r = reset_run_at("planned", false);
        let held_num = 62;
        r.play_to(T_SETTLED + 0.5);
        r.f.state = fixtures::reset_state(&r.f.state, "surprise", 2, now());
        let from_used = r.f.state.resets[0].from_used;
        r.frame();
        assert_eq!(r.resets().locks(), [Some(2), None]);
        r.play_to(T_SETTLED + 1.0);
        assert_eq!(
            r.f.widget.debug_frames()[0],
            Some(surprise_frame(500.0, from_used, held_num))
        );
        // Batch 1 would have released the ring at 2.15 s: the ring stays with batch 2.
        r.play_to(T_SETTLED + (PLANNED_END + 50.0) / 1000.0);
        assert_eq!(r.resets().locks(), [Some(2), None]);
        assert!(r.f.widget.debug_frames()[0].is_some());
        r.play_to(T_SETTLED + 0.5 + (SURPRISE_END + 10.0) / 1000.0);
        assert_eq!(r.resets().locks(), [None, None]);
        assert!(r.f.widget.debug_frames()[0].is_none());
        // One pill for each batch that reached its moment while owning a ring.
        assert_eq!(pill_shows(&r), 1);
    }

    #[test]
    fn stalled_lock_released_by_safety() {
        let mut r = reset_run_at("surprise", false);
        assert_eq!(r.resets().locks(), [Some(1), None]);
        assert_eq!(pill_shows(&r), 0);
        r.at(T_SETTLED + 5.0);
        assert_eq!(r.resets().locks(), [None, None]);
        assert_eq!(pill_shows(&r), 1);
        // The pill starts fresh at the late frame (Phase 1 shows it on release), so it is seen.
        let (_, pill) = r.resets().pill(r.f.t).expect("pill drawn after the stall");
        assert!(pill.alpha < 0.05, "{pill:?}");
        r.at(T_SETTLED + 6.0);
        assert!(r.resets().pill(r.f.t).is_some_and(|(_, p)| p.alpha > 0.99));
        assert!(r.f.widget.debug_frames()[0].is_none());
    }

    #[test]
    fn disconnect_mid_surprise_clears_effects() {
        let mut r = reset_run_at("surprise", false);
        r.at(T_SETTLED + 0.45);
        let t = r.f.t;
        assert!(r.resets().glow_alpha(t) > 0.0);
        assert!(!r.resets().sparks(t).is_empty());
        r.f.state = fixtures::scenario("disconnected", now());
        r.at(T_SETTLED + 0.5);
        let t = r.f.t;
        assert_eq!(r.resets().locks(), [None, None]);
        assert_eq!(r.resets().glow_alpha(t), 0.0);
        assert!(r.resets().sparks(t).is_empty());
        assert!(r.resets().pill(t).is_none());
        assert_eq!(r.frame().repaint, Repaint::Idle);
        // Nothing comes back with the rings, past the pill moment.
        r.f.state = fixtures::scenario("connected", now());
        r.at(T_SETTLED + 1.0);
        let t = r.f.t;
        assert_eq!(pill_shows(&r), 0);
        assert!(r.resets().pill(t).is_none());
        assert_eq!(r.resets().glow_alpha(t), 0.0);
    }

    #[test]
    fn reduced_motion_crossfades() {
        let mut r = reset_run_at("surprise", true);
        r.at(T_SETTLED + 0.1);
        let t = r.f.t;
        assert!(r.resets().sparks(t).is_empty());
        assert_eq!(r.resets().glow_alpha(t), 0.0);
        let f = r.f.widget.debug_frames()[0].clone().unwrap();
        assert_eq!((f.num, f.arc_fraction, f.num_blur), (62, 0.0, 0.0));
        r.at(T_SETTLED + 0.2);
        assert_eq!(r.f.widget.debug_frames()[0].as_ref().unwrap().num, 0);
        assert_eq!(pill_shows(&r), 1);
        r.at(T_SETTLED + 0.45);
        assert_eq!(r.resets().locks(), [Some(1), None]);
        assert!(r.resets().sparks(r.f.t).is_empty());
        r.at(T_SETTLED + 0.5);
        assert_eq!(r.resets().locks(), [None, None]);
    }

    #[test]
    fn planned_check_outlives_the_lock() {
        let mut r = reset_run_at("planned", false);
        r.play_to(T_SETTLED + 3.0);
        assert_eq!(r.resets().locks(), [None, None]);
        assert!(r.resets().ok(0, r.f.t).is_some_and(|(a, _)| a > 0.5));
        assert_eq!(r.frame().repaint, Repaint::Now);
        r.play_to(T_SETTLED + 5.0);
        assert!(r.resets().ok(0, r.f.t).is_none());
        assert_eq!(r.frame().repaint, Repaint::Idle);
    }

    #[test]
    fn surprise_settles_to_no_repaint() {
        let mut r = reset_run("surprise");
        assert_eq!(r.at(T_SETTLED + 5.0).repaint, Repaint::Idle);
    }

    #[test]
    fn mount_with_a_reset_seq_plays_nothing() {
        let mut f = fixture("connected", Lang::En);
        f.state = fixtures::reset_state(&f.state, "surprise", 4, now());
        let r = Run::new(f);
        assert_eq!(r.resets().locks(), [None, None]);
        assert_eq!(r.f.out.as_ref().unwrap().repaint, Repaint::Idle);
    }

    #[test]
    fn sparks_stay_inside_the_rounded_box() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), rings_size(false));
        // A spark in the top-left corner, off the rounded glass, is culled.
        assert_eq!(sparks::coverage(rect, pos2(1.0, 1.0), 2.5), 0.0);
        assert_eq!(sparks::coverage(rect, rect.center(), 2.5), 1.0);
        let c = sparks::coverage(rect, pos2(rect.center().x, 0.5), 2.5);
        assert!(c > 0.0 && c < 1.0, "{c}");
    }

    #[test]
    fn long_french_pill_shrinks_to_fit() {
        // "✓ Nouvelle semaine · limite hebdo remise à zéro" is wider than the widget at 10 px.
        let mut r = Run::new(fixture("connected", Lang::Fr));
        r.reset("plannedWeekly", 1);
        r.play_to(T_SETTLED + 2.5);
        let pill = r.f.widget.debug_rects()["pill"];
        // 1 px border + 4 px side margin on each side of the 244 px box.
        assert!(pill.left() >= 4.99 && pill.right() <= 239.01, "{pill:?}");
        assert!(pill.height() < 10.0 * 1.2 + 8.0, "font shrunk: {pill:?}");
        // The English one fits at 10 px.
        let mut r = Run::new(fixture("connected", Lang::En));
        r.reset("plannedWeekly", 1);
        r.play_to(T_SETTLED + 2.5);
        let pill = r.f.widget.debug_rects()["pill"];
        assert!(
            (pill.height() - (10.0 * 1.2 + 8.0)).abs() < 0.01,
            "{pill:?}"
        );
    }

    fn snap_reset(kind: &str, lang: Lang, ms: f64, name: &str) {
        let mut h = render("connected", lang, 1.0);
        let next = fixtures::reset_state(&h.state().state, kind, 1, now());
        h.state_mut().state = next;
        h.run();
        h.state_mut().t = T_SETTLED + ms / 1000.0;
        h.run();
        h.snapshot(name);
    }

    #[test]
    fn planned_t300_en() {
        snap_reset("planned", Lang::En, 300.0, "planned_t300_en");
    }
    #[test]
    fn planned_t900_en() {
        snap_reset("planned", Lang::En, 900.0, "planned_t900_en");
    }
    #[test]
    fn planned_t1600_en() {
        snap_reset("planned", Lang::En, 1600.0, "planned_t1600_en");
    }
    #[test]
    fn surprise_t200_en() {
        snap_reset("surprise", Lang::En, 200.0, "surprise_t200_en");
    }
    #[test]
    fn surprise_t500_en() {
        snap_reset("surprise", Lang::En, 500.0, "surprise_t500_en");
    }
    #[test]
    fn surprise_t1500_fr() {
        snap_reset("surprise", Lang::Fr, 1500.0, "surprise_t1500_fr");
    }
    #[test]
    fn entrance_t400_en() {
        let mut h = Harness::builder()
            .with_size(vec2(244.0, 146.0))
            .with_render_cursor(false)
            .with_context_setup(fonts::install)
            .build_ui_state(
                |ui, f: &mut Fixture| frame(ui.ctx(), f),
                fixture_live("connected", Lang::En),
            );
        let size = h.state().out.as_ref().expect("shown").size;
        h.set_size(size);
        h.run();
        h.state_mut().t = T_SETTLED + 0.4;
        h.run();
        h.snapshot("entrance_t400_en");
    }
}
