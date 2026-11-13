//! Hover controls (port of `HoverControls.tsx`, `Flags.tsx` and `index.css` `.ctl .ib .rf .flags`):
//! language flags top left, minimise and close top right, refresh bottom right (rings only).
//! They fade in over 0.2 s while the pointer is in the window and out when it leaves.
//!
//! Group opacity: CSS fades each control as one composited layer. Stacked egui shapes
//! faded one by one would let lower layers show through upper ones (flag stripes, icon
//! caps over their line). So every flag and icon is rasterized here into a small texture,
//! pixel-aligned at its current scale and turn, by supersampling its exact geometry (each
//! sample takes the topmost piece: no seams, no stacked anti-aliasing fringes). The texture
//! is then drawn once with a tint, which fades it exactly like CSS `opacity`.

use egui::epaint::Mesh;
use egui::{
    pos2, vec2, Color32, ColorImage, CornerRadius, CursorIcon, Event, Id, Painter, PointerButton,
    Pos2, Rect, Shape, TextureHandle, TextureId, TextureOptions, Vec2,
};
use limits_core::Command;

use super::draw::{self, Icon, FLAG_RADIUS};
use super::{colors, Action, FrameInput, Lang};
use crate::anim::easing::EASE_CSS;
use crate::anim::{refresh_press, REFRESH_END, REFRESH_SEND_AT};

/// The widget's 1 px border: absolute positions count from inside it.
const BORDER: f32 = 1.0;
/// `.ctl { top: 4px; right: 5px; gap: 2px }`, `.ib { 22×18, radius 5 }`.
const CTL_TOP: f32 = 4.0;
const CTL_RIGHT: f32 = 5.0;
const BTN_GAP: f32 = 2.0;
const BTN_SIZE: Vec2 = vec2(22.0, 18.0);
const BTN_RADIUS: u8 = 5;
/// `.rf { bottom: 5px; right: 5px }`.
const RF_BOTTOM: f32 = 5.0;
const RF_RIGHT: f32 = 5.0;
/// Lucide icon box (points) and stroke (viewBox units).
const ICON: f32 = 12.0;
const ICON_STROKE: f32 = 2.0;
/// `.flags { top: 6px; left: 8px; gap: 5px }`, each 16×11.
const FLAGS_TOP: f32 = 6.0;
const FLAGS_LEFT: f32 = 8.0;
const FLAG_GAP: f32 = 5.0;
const FLAG_SIZE: Vec2 = vec2(16.0, 11.0);
/// Inactive and hovered flag opacity (active is 1); hover scale.
const FLAG_ALPHA: f32 = 0.4;
const FLAG_ALPHA_HOVER: f32 = 0.8;
const FLAG_SCALE_HOVER: f32 = 1.1;
/// Outline widths, inactive and active, CSS px.
const FLAG_OUTLINE_W: f32 = 1.0;
const FLAG_OUTLINE_ACTIVE_W: f32 = 1.5;
/// Transition times, seconds.
const FADE: f32 = 0.2;
const FLAG_ANIM: f32 = 0.15;
const BTN_ANIM: f32 = 0.12;
/// Samples per pixel side when rasterizing a control.
const SS: usize = 5;

/// The window buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctl {
    Minimize,
    Close,
    Refresh,
}

impl Ctl {
    pub fn name(self) -> &'static str {
        match self {
            Ctl::Minimize => "ctl_minimize",
            Ctl::Close => "ctl_close",
            Ctl::Refresh => "ctl_refresh",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Ctl::Minimize => Icon::Minus,
            Ctl::Close => Icon::X,
            Ctl::Refresh => Icon::RefreshCw,
        }
    }

    /// Raster slot (0 and 1 are the flags).
    fn slot(self) -> usize {
        match self {
            Ctl::Minimize => 2,
            Ctl::Close => 3,
            Ctl::Refresh => 4,
        }
    }
}

/// What the controls did this frame.
pub struct ControlsOut {
    pub actions: Vec<Action>,
    /// Rects a press must not start a window drag from.
    pub nodrag: Vec<Rect>,
    pub animating: bool,
}

/// A rasterized control and what it was rasterized for.
struct Raster {
    key: Vec<u32>,
    tex: TextureHandle,
    quad: Rect,
}

#[derive(Default)]
pub struct Controls {
    /// Animation clock of the last accepted refresh click, while its press plays.
    refresh_at: Option<f64>,
    refresh_sent: bool,
    /// FR flag, UK flag, minimise, close, refresh.
    rasters: [Option<Raster>; 5],
}

/// CSS `ease`, exact at both ends so a finished transition reads as settled.
fn ease_css(t: f32) -> f32 {
    if t <= 0.0 {
        0.0
    } else if t >= 1.0 {
        1.0
    } else {
        EASE_CSS.apply(t)
    }
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|f| f.to_bits()).collect()
}

fn premul(c: Color32) -> [f32; 4] {
    [c.r(), c.g(), c.b(), c.a()].map(f32::from)
}

/// Rasterizes `sample` (premultiplied color at a point) over `bounds` snapped out to whole
/// pixels; returns the image and the quad (points) it covers, one texel per pixel.
fn raster(bounds: Rect, ppp: f32, sample: &dyn Fn(Pos2) -> [f32; 4]) -> (ColorImage, Rect) {
    let (x0, y0) = ((bounds.min.x * ppp).floor(), (bounds.min.y * ppp).floor());
    let (x1, y1) = ((bounds.max.x * ppp).ceil(), (bounds.max.y * ppp).ceil());
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let n = (SS * SS) as f32;
    let at = |base: f32, i: usize, s: usize| (base + i as f32 + (s as f32 + 0.5) / SS as f32) / ppp;
    let mut pixels = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let c = sample(pos2(at(x0, i, sx), at(y0, j, sy)));
                    for (a, c) in acc.iter_mut().zip(c) {
                        *a += c;
                    }
                }
            }
            let [r, g, b, a] = acc.map(|v| (v / n).round().clamp(0.0, 255.0) as u8);
            pixels.push(Color32::from_rgba_premultiplied(r, g, b, a));
        }
    }
    let quad = Rect::from_min_max(pos2(x0 / ppp, y0 / ppp), pos2(x1 / ppp, y1 / ppp));
    (ColorImage::new([w, h], pixels), quad)
}

/// Signed distance to the rounded rect (negative inside).
fn rrect_sdf(r: Rect, radius: f32, q: Pos2) -> f32 {
    let half = r.size() / 2.0 - Vec2::splat(radius);
    let d = (q - r.center()).abs() - half;
    vec2(d.x.max(0.0), d.y.max(0.0)).length() + d.x.max(d.y).min(0.0) - radius
}

fn inside_convex(poly: &[Pos2], q: Pos2) -> bool {
    let mut sign = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let e = b - a;
        let c = e.x * (q.y - a.y) - e.y * (q.x - a.x);
        if c != 0.0 {
            if sign == 0.0 {
                sign = c.signum();
            } else if c.signum() != sign {
                return false;
            }
        }
    }
    true
}

fn seg_dist(a: Pos2, b: Pos2, q: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 {
        ((q - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(q)
}

/// A CSS border width in points: rounded down to whole device pixels, at least one.
fn snapped_width(w: f32, ppp: f32) -> f32 {
    (w * ppp).floor().max(1.0) / ppp
}

/// The flag drawn in `rect` (already scaled by `scale`): its pieces clipped to the 2 px
/// rounded box and the fitted viewBox, and its outline (width before scaling, color).
fn flag_image(
    uk: bool,
    rect: Rect,
    scale: f32,
    outline: (f32, Color32),
    ppp: f32,
) -> (ColorImage, Rect) {
    let pieces = if uk {
        draw::flag_uk_pieces(rect)
    } else {
        draw::flag_fr_pieces(rect)
    };
    let view = draw::flag_view(rect, uk);
    let radius = FLAG_RADIUS * scale;
    let ow = snapped_width(outline.0, ppp) * scale;
    let outer = rect.expand(ow);
    let line = premul(outline.1);
    let sample = |q: Pos2| -> [f32; 4] {
        if rrect_sdf(rect, radius, q) <= 0.0 {
            if !view.contains(q) {
                return [0.0; 4];
            }
            let top = pieces.iter().rev().find(|(poly, _)| inside_convex(poly, q));
            top.map_or([0.0; 4], |(_, c)| premul(*c))
        } else if rrect_sdf(outer, radius + ow, q) <= 0.0 {
            line
        } else {
            [0.0; 4]
        }
    };
    raster(outer.expand(1.0), ppp, &sample)
}

/// White coverage mask of `icon` fitted in `rect`, scaled by `scale` and turned by `turns`
/// around its center. Round caps and joins come from the distance to the path.
fn icon_image(icon: Icon, rect: Rect, scale: f32, turns: f32, ppp: f32) -> (ColorImage, Rect) {
    let s = rect.width().min(rect.height()) / 24.0 * scale;
    let rot = egui::emath::Rot2::from_angle(std::f32::consts::TAU * turns);
    let c = rect.center();
    let paths: Vec<Vec<Pos2>> = draw::icon_paths(icon)
        .into_iter()
        .map(|path| {
            path.into_iter()
                .map(|q| c + rot * ((q - pos2(12.0, 12.0)) * s))
                .collect()
        })
        .collect();
    let hw = ICON_STROKE * s / 2.0;
    let sample = |q: Pos2| -> [f32; 4] {
        let hit = paths
            .iter()
            .any(|path| path.windows(2).any(|w| seg_dist(w[0], w[1], q) <= hw));
        if hit {
            [255.0; 4]
        } else {
            [0.0; 4]
        }
    };
    // A turned icon reaches 12·√2 viewBox units from its center, plus half the stroke.
    let reach = 12.0 * s * std::f32::consts::SQRT_2 + hw;
    raster(
        Rect::from_center_size(c, Vec2::splat(2.0 * reach)),
        ppp,
        &sample,
    )
}

/// Whether this frame has a release of the primary button.
fn primary_released(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }
            )
        })
    })
}

impl Controls {
    /// The texture of `slot` for `key`, rasterized again only when `key` changes.
    fn texture(
        &mut self,
        ctx: &egui::Context,
        slot: usize,
        key: Vec<u32>,
        make: impl FnOnce() -> (ColorImage, Rect),
    ) -> (TextureId, Rect) {
        let raster = &mut self.rasters[slot];
        match raster {
            Some(r) if r.key == key => {}
            Some(r) => {
                let (image, quad) = make();
                r.tex.set(image, TextureOptions::LINEAR);
                r.key = key;
                r.quad = quad;
            }
            None => {
                let (image, quad) = make();
                let name = format!("limitrings.ctl.{slot}");
                let tex = ctx.load_texture(name, image, TextureOptions::LINEAR);
                *raster = Some(Raster { key, tex, quad });
            }
        }
        let r = raster.as_ref().expect("just filled");
        (r.tex.id(), r.quad)
    }

    fn blit(p: &Painter, tex: TextureId, quad: Rect, tint: Color32) {
        let mut mesh = Mesh::with_texture(tex);
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        mesh.add_rect_with_uv(quad, uv, tint);
        p.add(Shape::mesh(mesh));
    }

    /// Paints the controls over the widget box `rect`, handles their clicks and sends a
    /// pending refresh. `note` records the controls' rects by name.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        p: &Painter,
        rect: Rect,
        input: &FrameInput,
        show_refresh: bool,
        note: &mut dyn FnMut(&'static str, Rect),
    ) -> ControlsOut {
        let pointer = ctx.pointer_hover_pos();
        let released = primary_released(ctx);
        let hovers = |r: Rect| pointer.is_some_and(|q| r.contains(q));
        let mut actions = vec![];
        let mut nodrag = vec![];
        let mut animating = false;
        // An eased 0..1 toward `on`; anything short of it keeps frames coming.
        let mut animate = |id: Id, on: bool, secs: f32| {
            let v = ctx.animate_bool_with_time_and_easing(id, on, secs, ease_css);
            animating |= v != if on { 1.0 } else { 0.0 };
            v
        };

        let inside = pointer.is_some();
        let alpha = animate(Id::unique("limitrings.ctl.fade"), inside, FADE);

        // Flags, top left. The whole flags box is no-drag (`data-nodrag`), gap included.
        let inner = rect.shrink(BORDER);
        let lang = input.prefs.lang;
        let flags_min = inner.min + vec2(FLAGS_LEFT, FLAGS_TOP);
        let flags_size = vec2(2.0 * FLAG_SIZE.x + FLAG_GAP, FLAG_SIZE.y);
        nodrag.push(Rect::from_min_size(flags_min, flags_size));
        let mut flags = vec![];
        for (i, flag) in [Lang::Fr, Lang::En].into_iter().enumerate() {
            let offset = vec2(i as f32 * (FLAG_SIZE.x + FLAG_GAP), 0.0);
            let r = Rect::from_min_size(flags_min + offset, FLAG_SIZE);
            note(if i == 0 { "flag_fr" } else { "flag_en" }, r);
            let active = flag == lang;
            let hover = hovers(r);
            if hover {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
                if released && !active {
                    actions.push(Action::SetLang(flag));
                }
            }
            let hk = animate(Id::unique(("limitrings.flag.hover", i)), hover, FLAG_ANIM);
            let ak = animate(Id::unique(("limitrings.flag.on", i)), active, FLAG_ANIM);
            flags.push((i, flag, r, active, hk, ak));
        }

        // Minimise and close, top right; refresh, bottom right.
        let top_right = pos2(inner.right() - CTL_RIGHT, inner.top() + CTL_TOP);
        let close = Rect::from_min_size(top_right - vec2(BTN_SIZE.x, 0.0), BTN_SIZE);
        let minimize = close.translate(vec2(-(BTN_SIZE.x + BTN_GAP), 0.0));
        let mut buttons = vec![(Ctl::Minimize, minimize), (Ctl::Close, close)];
        if show_refresh {
            let corner = pos2(inner.right() - RF_RIGHT, inner.bottom() - RF_BOTTOM);
            buttons.push((Ctl::Refresh, Rect::from_min_max(corner - BTN_SIZE, corner)));
        }
        let t = input.t;
        let mut shown_buttons = vec![];
        for (ctl, r) in buttons {
            note(ctl.name(), r);
            nodrag.push(r);
            let hover = hovers(r);
            if hover {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
                if released {
                    match ctl {
                        Ctl::Minimize => actions.push(Action::Minimize),
                        Ctl::Close => actions.push(Action::Quit),
                        Ctl::Refresh => {
                            let busy = self
                                .refresh_at
                                .is_some_and(|at| (t - at) * 1000.0 < REFRESH_END);
                            if !busy {
                                self.refresh_at = Some(t);
                                self.refresh_sent = false;
                            }
                        }
                    }
                }
            }
            let k = animate(
                Id::unique(("limitrings.ctl.hover", ctl.slot())),
                hover,
                BTN_ANIM,
            );
            shown_buttons.push((ctl, r, k));
        }

        // The request goes out while the icon is still turning; clicks wait for the end.
        let mut press = (1.0, 0.0);
        if let Some(at) = self.refresh_at {
            let ms = (t - at) * 1000.0;
            if !self.refresh_sent && ms >= REFRESH_SEND_AT {
                self.refresh_sent = true;
                actions.push(Action::Send(Command::Refresh));
            }
            if ms >= REFRESH_END {
                self.refresh_at = None;
            } else {
                press = refresh_press(ms.max(0.0));
                animating = true;
            }
        }

        if alpha > 0.0 {
            for (i, flag, r, active, hk, ak) in flags {
                self.paint_flag(ctx, p, i, flag == Lang::En, r, active, hk, ak, alpha);
            }
            for (ctl, r, k) in shown_buttons {
                let (scale, turns) = if ctl == Ctl::Refresh {
                    press
                } else {
                    (1.0, 0.0)
                };
                self.paint_button(ctx, p, ctl, r, k, scale, turns, alpha);
            }
        }

        ControlsOut {
            actions,
            nodrag,
            animating,
        }
    }

    /// One flag: opacity .4 (hover .8, active 1), hover scale 1.1, outline by state.
    #[expect(clippy::too_many_arguments)]
    fn paint_flag(
        &mut self,
        ctx: &egui::Context,
        p: &Painter,
        slot: usize,
        uk: bool,
        r: Rect,
        active: bool,
        hover_k: f32,
        active_k: f32,
        alpha: f32,
    ) {
        let ppp = p.pixels_per_point();
        let opacity = egui::lerp(
            egui::lerp(FLAG_ALPHA..=FLAG_ALPHA_HOVER, hover_k)..=1.0,
            active_k,
        );
        let scale = egui::lerp(1.0..=FLAG_SCALE_HOVER, hover_k);
        let shown = Rect::from_center_size(r.center(), r.size() * scale);
        let outline = if active {
            (FLAG_OUTLINE_ACTIVE_W, colors::FLAG_OUTLINE_ACTIVE)
        } else {
            (FLAG_OUTLINE_W, colors::FLAG_OUTLINE)
        };
        let key = bits(&[
            ppp,
            shown.min.x,
            shown.min.y,
            shown.max.x,
            shown.max.y,
            if active { 1.0 } else { 0.0 },
        ]);
        let (tex, quad) = self.texture(ctx, slot, key, || {
            flag_image(uk, shown, scale, outline, ppp)
        });
        Self::blit(p, tex, quad, Color32::WHITE.gamma_multiply(opacity * alpha));
    }

    /// One button: hover background (red for close), icon mute → white, press scale and turn.
    #[expect(clippy::too_many_arguments)]
    fn paint_button(
        &mut self,
        ctx: &egui::Context,
        p: &Painter,
        ctl: Ctl,
        r: Rect,
        hover_k: f32,
        scale: f32,
        turns: f32,
        alpha: f32,
    ) {
        let ppp = p.pixels_per_point();
        if hover_k > 0.0 {
            let bg = if ctl == Ctl::Close {
                colors::CLOSE_HOVER
            } else {
                colors::BTN_HOVER_BG
            };
            let shown = Rect::from_center_size(r.center(), r.size() * scale);
            let fill = bg.gamma_multiply(hover_k * alpha);
            p.rect_filled(shown, CornerRadius::same(BTN_RADIUS), fill);
        }
        let icon_rect = Rect::from_center_size(r.center(), Vec2::splat(ICON));
        let key = bits(&[ppp, icon_rect.min.x, icon_rect.min.y, scale, turns]);
        let (tex, quad) = self.texture(ctx, ctl.slot(), key, || {
            icon_image(ctl.icon(), icon_rect, scale, turns, ppp)
        });
        let color = colors::MUTE.lerp_to_gamma(Color32::WHITE, hover_k);
        Self::blit(p, tex, quad, color.gamma_multiply(alpha));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::widget::snapshot_tests::{render, render_dt};
    use crate::ui::Lang;
    use egui::{pos2, Pos2};

    /// Between the two ring columns: plain glass.
    const RINGS_CENTER: Pos2 = pos2(122.0, 60.0);
    /// The French flag (left 1 + 8, top 1 + 6, 16×11).
    const FLAG_FR: Pos2 = pos2(17.0, 12.5);

    fn name(ctl: Ctl) -> &'static str {
        ctl.name()
    }

    type H = egui_kittest::Harness<'static, crate::ui::widget::snapshot_tests::Fixture>;

    /// Moves the pointer to `at`, presses and releases `button` in one frame; that frame's actions.
    fn press(h: &mut H, at: Pos2, button: egui::PointerButton) -> Vec<Action> {
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
        h.state().out.as_ref().expect("shown").actions.clone()
    }

    fn hovered(kind: &str, lang: Lang) -> H {
        let mut h = render(kind, lang, 1.0);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(RINGS_CENTER));
        h.run();
        h
    }

    fn click_ctl(kind: &str, ctl: Ctl) -> Vec<Action> {
        let mut h = hovered(kind, Lang::En);
        let at = h.state().widget.debug_rects()[name(ctl)].center();
        press(&mut h, at, egui::PointerButton::Primary)
    }

    /// Actions of the click frame, of a frame 400 ms later, and of a second click at 500 ms.
    fn refresh_sequence(kind: &str) -> (Vec<Action>, Vec<Action>, Vec<Action>) {
        let mut h = hovered(kind, Lang::En);
        let at = h.state().widget.debug_rects()[name(Ctl::Refresh)].center();
        let t0 = h.state().t;
        let at_0 = press(&mut h, at, egui::PointerButton::Primary);
        h.state_mut().t = t0 + 0.4;
        h.step();
        let at_400 = h.state().out.as_ref().expect("shown").actions.clone();
        h.state_mut().t = t0 + 0.5;
        let again = press(&mut h, at, egui::PointerButton::Primary);
        (at_0, at_400, again)
    }

    fn press_at(kind: &str, at: Pos2) -> Vec<Action> {
        let mut h = render(kind, Lang::En, 1.0);
        press(&mut h, at, egui::PointerButton::Primary)
    }

    fn right_press_at(kind: &str, at: Pos2) -> Vec<Action> {
        let mut h = render(kind, Lang::En, 1.0);
        press(&mut h, at, egui::PointerButton::Secondary)
    }

    #[test]
    fn close_quits() {
        assert_eq!(click_ctl("connected", Ctl::Close), vec![Action::Quit]);
    }

    #[test]
    fn minimize_minimizes() {
        assert_eq!(
            click_ctl("connected", Ctl::Minimize),
            vec![Action::Minimize]
        );
        assert_eq!(
            click_ctl("disconnected", Ctl::Minimize),
            vec![Action::Minimize]
        );
    }

    #[test]
    fn refresh_sends_after_350ms_once() {
        let (at_0, at_400, again_at_500) = refresh_sequence("connected");
        assert!(at_0.is_empty());
        assert_eq!(at_400, vec![Action::Send(Command::Refresh)]);
        assert!(again_at_500.is_empty());
    }

    #[test]
    fn refresh_only_with_rings() {
        let h = hovered("disconnected", Lang::En);
        assert!(!h.state().widget.debug_rects().contains_key("ctl_refresh"));
        let h = hovered("connected", Lang::En);
        assert!(h.state().widget.debug_rects().contains_key("ctl_refresh"));
    }

    #[test]
    fn press_on_glass_starts_drag_but_not_on_flags() {
        assert_eq!(press_at("connected", RINGS_CENTER), vec![Action::StartDrag]);
        assert_eq!(
            press_at("connected", FLAG_FR),
            vec![Action::SetLang(Lang::Fr)]
        );
    }

    #[test]
    fn active_flag_does_nothing() {
        // The English flag: 5 right of the French one.
        assert!(press_at("connected", FLAG_FR + egui::vec2(21.0, 0.0)).is_empty());
    }

    #[test]
    fn buttons_and_panel_button_never_drag() {
        let h = render("disconnected", Lang::En, 1.0);
        let button = h.state().widget.debug_rects()["connect_button"].center();
        assert_eq!(
            press_at("disconnected", button),
            vec![Action::Send(Command::Connect)]
        );
        // Between the flags (the flags box is no-drag as a whole, like `data-nodrag`).
        assert!(press_at("connected", FLAG_FR + egui::vec2(10.5, 0.0)).is_empty());
    }

    #[test]
    fn right_click_never_drags() {
        assert!(!right_press_at("connected", RINGS_CENTER).contains(&Action::StartDrag));
    }

    #[test]
    fn fade_repaints_until_settled() {
        let mut h = render_dt("connected", Lang::En, 1.0, 0.1);
        assert_eq!(
            h.state().out.as_ref().unwrap().repaint,
            crate::ui::Repaint::Idle
        );
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(RINGS_CENTER));
        // egui counts the frame that starts a fade as one step in: 0.1 of 0.2 s.
        h.step();
        assert_eq!(
            h.state().out.as_ref().unwrap().repaint,
            crate::ui::Repaint::Now
        );
        h.run();
        assert_eq!(
            h.state().out.as_ref().unwrap().repaint,
            crate::ui::Repaint::Idle
        );
    }

    /// The hovered widget through the window's CPU rasterizer (`render.rs`) matches the
    /// wgpu snapshot: the control textures and their tints go through both alike.
    #[test]
    fn hover_soft_render_matches_snapshot() {
        use crate::ui::widget::snapshot_tests::{fixture, frame};
        let ctx = egui::Context::default();
        crate::ui::fonts::install(&ctx);
        let (w, h) = (244u32, 146u32);
        let mut renderer = crate::render::SoftRenderer::default();
        let mut canvas = vec![0u32; (w * h) as usize];
        let mut f = fixture("connected", Lang::En);
        for step in 0..8 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    egui::vec2(244.0, 145.8),
                )),
                time: Some(f64::from(step) * 0.1),
                ..Default::default()
            };
            input
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .native_pixels_per_point = Some(1.0);
            input.events.push(egui::Event::PointerMoved(RINGS_CENTER));
            let out = ctx.run_pass(input, |ctx| frame(ctx, &mut f));
            let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
            renderer.render(
                &mut canvas,
                w as usize,
                h as usize,
                &primitives,
                out.textures_delta,
                out.pixels_per_point,
            );
        }
        let soft = image::RgbaImage::from_fn(w, h, |x, y| {
            let [b, g, r, a] = canvas[(y * w + x) as usize].to_le_bytes();
            image::Rgba([r, g, b, a])
        });
        let snapshot = image::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/hover_controls_connected_en.png"
        ))
        .unwrap()
        .to_rgba8();
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

    /// Pointer on close (red background, white icon), then on the inactive French flag
    /// (opacity .8, scale 1.1).
    #[test]
    fn hover_close_and_flag_en() {
        let mut h = hovered("connected", Lang::En);
        let close = h.state().widget.debug_rects()["ctl_close"].center();
        h.input_mut().events.push(egui::Event::PointerMoved(close));
        h.run();
        h.snapshot("hover_close_en");
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(FLAG_FR));
        h.run();
        h.snapshot("hover_flag_fr_en");
    }

    #[test]
    fn hover_controls_connected_en() {
        hovered("connected", Lang::En).snapshot("hover_controls_connected_en");
    }

    #[test]
    fn hover_controls_disconnected_fr() {
        hovered("disconnected", Lang::Fr).snapshot("hover_controls_disconnected_fr");
    }

    #[test]
    fn hover_controls_error_en() {
        hovered("error", Lang::En).snapshot("hover_controls_error_en");
    }

    /// Halfway through the 0.2 s fade-in: no darker dots at icon ends or flag seams.
    #[test]
    fn hover_controls_midfade_en() {
        // egui counts the frame that starts a fade as one step in: 0.06 s of 0.2, which
        // CSS `ease` puts at about half opacity.
        let mut h = render_dt("connected", Lang::En, 1.0, 0.06);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(RINGS_CENTER));
        h.step();
        h.snapshot("hover_controls_midfade_en");
    }

    #[test]
    fn hover_controls_connected_en_150() {
        let mut h = render("connected", Lang::En, 1.5);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(RINGS_CENTER));
        h.run();
        h.snapshot("hover_controls_connected_en@1.5x");
    }
}
