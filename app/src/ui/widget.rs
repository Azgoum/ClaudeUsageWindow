//! The widget: glass box, rings view (connected) and the connect panels (`connect.rs`).
//!
//! Layout in CSS px = egui points (`ui/src/index.css` `.wd`): 1 px border; rings row padding
//! 24 top, 14 sides, 22 bottom, two 104 wide columns 6 apart → box 244 wide.

#[cfg(test)]
use std::collections::BTreeMap;

use egui::{pos2, vec2, Color32, CursorIcon, LayerId, Painter, Rect, Vec2};
use limits_core::{Conn, LimitKey, RingView};

use super::connect::{Panel, PANEL_W};
use super::controls::Controls;
use super::input::PressTracker;
use super::ring::{err_line, is_near_limit};
use super::rings::{self, Column, RingTweens, COLUMN_H, COLUMN_W, LINE_HEIGHT};
use super::{colors, draw, fonts, Action, FrameInput, FrameOutput, Repaint};
use crate::anim::RingFrame;

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

pub struct Widget {
    rings: [RingTweens; 2],
    controls: Controls,
    presses: PressTracker,
    #[cfg(test)]
    debug_rects: BTreeMap<&'static str, Rect>,
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
            controls: Controls::default(),
            presses: PressTracker::default(),
            #[cfg(test)]
            debug_rects: BTreeMap::new(),
        }
    }

    /// Rects of the clickable parts drawn by the last `show`, by name.
    #[cfg(test)]
    pub fn debug_rects(&self) -> &BTreeMap<&'static str, Rect> {
        &self.debug_rects
    }

    #[cfg_attr(not(test), expect(unused_variables))]
    fn note(&mut self, name: &'static str, rect: Rect) {
        #[cfg(test)]
        self.debug_rects.insert(name, rect);
    }

    /// Paints one frame and reports what the host must do.
    pub fn show(&mut self, ctx: &egui::Context, input: &FrameInput) -> FrameOutput {
        #[cfg(test)]
        self.debug_rects.clear();
        let clicks = self.presses.frame(ctx);
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

        let mut animating = false;
        let mut actions = vec![];
        let mut nodrag = vec![];
        match (rings, &panel) {
            (Some(rings), _) => {
                animating = self.paint_rings(&painter, rect, rings, [None, None], input);
            }
            (None, panel) => {
                self.rings.iter_mut().for_each(RingTweens::reset);
                if let Some(panel) = panel {
                    let mut noted = vec![];
                    let done = panel.paint(ctx, &painter, rect, input, &clicks, &mut |n, r| {
                        noted.push((n, r));
                    });
                    for (n, r) in noted {
                        self.note(n, r);
                    }
                    actions = done.actions;
                    animating = done.animating;
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
        // A primary press on the glass, off every control, moves the window.
        if clicks
            .press
            .is_some_and(|q| rect.contains(q) && !nodrag.iter().any(|r| r.contains(q)))
        {
            actions.push(Action::StartDrag);
        }
        FrameOutput {
            actions,
            size,
            extra: 0.0,
            repaint: if animating {
                Repaint::Now
            } else {
                Repaint::Idle
            },
        }
    }

    /// The rings row (and the error foot line). `frames` override each ring while a reset
    /// animation plays. Returns whether a transition is still running.
    fn paint_rings(
        &mut self,
        p: &Painter,
        rect: Rect,
        rings: &[RingView; 2],
        frames: [Option<&RingFrame>; 2],
        input: &FrameInput,
    ) -> bool {
        let lang = input.prefs.lang;
        let error = input.state.error.as_ref();
        let fx: &dyn Fn(Color32) -> Color32 = if error.is_some() { &gray_half } else { &|c| c };
        let row_top = rect.top() + BORDER + ROW_PAD_TOP;
        let mut animating = false;
        for (i, view) in rings.iter().enumerate() {
            let tweens = &mut self.rings[i];
            tweens.update(view, input.t);
            animating |= tweens.animating(input.t);
            let x = rect.left() + BORDER + ROW_PAD_SIDE + i as f32 * (COLUMN_W + ROW_GAP);
            let col_rect = rings::column_rect(pos2(x, row_top));
            let col = Column {
                view,
                live: tweens.value(input.t),
                frame: frames[i],
                lang,
                now: input.now,
                tz: input.tz,
            };
            rings::paint_column(p, col_rect, &col, fx);
            let name = match view.key {
                LimitKey::Session => "ring_session",
                LimitKey::Weekly => "ring_weekly",
            };
            self.note(name, col_rect);
        }
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

    fn input<'a>(state: &'a AppState, prefs: &'a Prefs, t: f64) -> FrameInput<'a> {
        FrameInput {
            state,
            prefs,
            now: now(),
            tz: fixtures::tz(),
            t,
            reduced_motion: false,
            offset_y: 0.0,
            focused: true,
        }
    }

    pub(crate) struct Fixture {
        pub widget: Widget,
        pub state: AppState,
        pub prefs: Prefs,
        pub out: Option<FrameOutput>,
        /// Animation clock of the next frame.
        pub t: f64,
    }

    /// Paints the backdrop and the widget, settled, into a pass of `ctx`.
    pub(crate) fn frame(ctx: &egui::Context, f: &mut Fixture) {
        ctx.layer_painter(LayerId::background())
            .rect_filled(ctx.content_rect(), 0, BACKDROP);
        let out = f.widget.show(ctx, &input(&f.state, &f.prefs, f.t));
        f.out = Some(out);
    }

    pub(crate) fn fixture(kind: &str, lang: Lang) -> Fixture {
        Fixture {
            widget: Widget::new(),
            state: fixtures::scenario(kind, now()),
            prefs: prefs(lang),
            out: None,
            t: T_SETTLED,
        }
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
}
