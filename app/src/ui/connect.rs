//! The panels of every non-ring state (port of `ConnectPanel.tsx` and `.wd .st`): connect,
//! waiting, Firefox needed, session expired, and the lone error line of a connected widget
//! that has no rings yet.
//!
//! Layout in CSS px = egui points: 1 px border, padding 26 top / 18 sides / 14 bottom.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Color32, CursorIcon, FontId, Galley, Painter, Rect, Stroke};
use limits_core::{Command, Conn};

use super::i18n::t;
use super::ring::err_line;
use super::rings::{baseline, line_galley_in};
use super::{colors, draw, fonts, Action, FrameInput};

pub const BORDER: f32 = 1.0;
const PAD_TOP: f32 = 26.0;
const PAD_SIDE: f32 = 18.0;
const PAD_BOTTOM: f32 = 14.0;
/// `.wd .st { width: 242px }`.
pub const PANEL_W: f32 = 242.0;
const CONTENT_W: f32 = PANEL_W - 2.0 * PAD_SIDE;
const TITLE_SIZE: f32 = 13.0;
const TITLE_GAP: f32 = 3.0;
const BODY_SIZE: f32 = 11.0;
const BODY_LINE: f32 = BODY_SIZE * 1.45;
const BODY_GAP: f32 = 10.0;
const BTN_PAD: f32 = 9.0;
const BTN_SIZE: f32 = 12.5;
const BTN_RADIUS: f32 = 10.0;
const BTN_HOVER: f32 = 1.06;
const SPIN_SIZE: f32 = 12.0;
const SPIN_GAP: f32 = 6.0;
const LINK_UNDERLINE_OFFSET: f32 = 2.0;
const FIREFOX_URL: &str = "https://www.mozilla.org/firefox/";

enum Foot {
    None,
    Button(Arc<Galley>, Action),
    Links([(Arc<Galley>, Action); 2]),
}

/// A laid-out panel: galleys and the box height they make.
pub struct Panel {
    title: Option<Arc<Galley>>,
    spinner: bool,
    body: Option<(Arc<Galley>, Color32)>,
    foot: Foot,
    /// Whole box height incl. border and padding.
    pub height: f32,
}

/// What painting a panel produced.
pub struct Painted {
    pub actions: Vec<Action>,
    pub animating: bool,
}

fn wrapped(p: &Painter, text: &str, color: Color32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: fonts::regular(BODY_SIZE),
            color,
            line_height: Some(BODY_LINE),
            ..Default::default()
        },
    );
    job.wrap.max_width = CONTENT_W;
    p.layout_job(job)
}

fn single(p: &Painter, text: &str, font: FontId, color: Color32, h: f32) -> Arc<Galley> {
    line_galley_in(p, text, font, color, h)
}

impl Panel {
    /// Lays the panel for `input` out; `p` only provides the fonts.
    pub fn layout(p: &Painter, input: &FrameInput) -> Panel {
        let l = t(input.prefs.lang);
        let state = input.state;
        let title_h = TITLE_SIZE * 1.2;
        let title = |text: &str| single(p, text, fonts::bold(TITLE_SIZE), colors::INK, title_h);
        let body = |text: &str, c| Some((wrapped(p, text, c), c));
        let button = |label: &str, action| {
            Foot::Button(
                single(
                    p,
                    label,
                    fonts::semibold(BTN_SIZE),
                    colors::BTN_LABEL,
                    BTN_SIZE * 1.2,
                ),
                action,
            )
        };
        let connect = Action::Send(Command::Connect);
        let mut panel = Panel {
            title: None,
            spinner: false,
            body: None,
            foot: Foot::None,
            height: 0.0,
        };
        match &state.conn {
            Conn::Connected => {
                if let Some(err) = &state.error {
                    let line = err_line(err.retry_at, input.now, input.prefs.lang);
                    panel.body = body(&line, colors::ERR_TEXT);
                }
            }
            Conn::Waiting => {
                panel.title = Some(title(l.w_title));
                panel.spinner = true;
                panel.body = body(&(l.w_other)("Firefox"), colors::MUTE);
                let link = |s: &str| {
                    single(
                        p,
                        s,
                        fonts::regular(BODY_SIZE),
                        colors::LINK,
                        BODY_SIZE * 1.2,
                    )
                };
                panel.foot = Foot::Links([
                    (link(l.again), connect),
                    (link(l.cancel), Action::Send(Command::CancelConnect)),
                ]);
            }
            Conn::NeedsFirefox => {
                panel.title = Some(title(l.need_firefox_title));
                panel.body = body(l.need_firefox_text, colors::MUTE);
                panel.foot = button(l.get_firefox, Action::OpenUrl(FIREFOX_URL));
            }
            Conn::Expired => {
                panel.title = Some(title(l.e_title));
                panel.body = body(l.e_text, colors::MUTE);
                panel.foot = button(l.e_btn, connect);
            }
            Conn::Disconnected { timed_out } => {
                panel.title = Some(title(l.c_title));
                let text = if *timed_out { l.timed_out } else { l.c_text };
                panel.body = body(text, colors::MUTE);
                panel.foot = button(l.c_btn, connect);
            }
        }
        let mut h = 0.0;
        if let Some(t) = &panel.title {
            h += t.size().y + TITLE_GAP;
        }
        if let Some((b, _)) = &panel.body {
            h += b.size().y + BODY_GAP;
        }
        h += match &panel.foot {
            Foot::None => 0.0,
            Foot::Button(g, _) => g.size().y + 2.0 * BTN_PAD,
            Foot::Links([(g, _), _]) => g.size().y,
        };
        if panel.title.is_none() && panel.body.is_none() {
            h = 0.0;
        }
        panel.height = 2.0 * BORDER + PAD_TOP + PAD_BOTTOM + h;
        panel
    }

    /// Paints into the box `rect` (as sized by `layout`). `note` records named rects.
    pub fn paint(
        &self,
        ctx: &egui::Context,
        p: &Painter,
        rect: Rect,
        input: &FrameInput,
        note: &mut dyn FnMut(&'static str, Rect),
    ) -> Painted {
        let left = rect.left() + BORDER + PAD_SIDE;
        let mut y = rect.top() + BORDER + PAD_TOP;
        let pointer = ctx.pointer_hover_pos();
        let released = ctx.input(|i| i.pointer.primary_released());
        let mut actions = vec![];
        let mut hit = |r: Rect, action: &Action| -> bool {
            let hover = pointer.is_some_and(|q| r.contains(q));
            if hover {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
                if released {
                    actions.push(action.clone());
                }
            }
            hover
        };

        if let Some(g) = &self.title {
            let mut x = left;
            if self.spinner {
                let c = pos2(x + SPIN_SIZE / 2.0, y + baseline(g) - 4.0);
                draw::spinner(p, c, input.t, input.reduced_motion);
                x += SPIN_SIZE + SPIN_GAP;
            }
            p.galley(pos2(x, y), g.clone(), colors::INK);
            y += g.size().y + TITLE_GAP;
        }
        if let Some((g, color)) = &self.body {
            p.galley(pos2(left, y), g.clone(), *color);
            note("body", Rect::from_min_size(pos2(left, y), g.size()));
            y += g.size().y + BODY_GAP;
        }
        match &self.foot {
            Foot::None => {}
            Foot::Button(label, action) => {
                let r = Rect::from_min_size(
                    pos2(left, y),
                    vec2(CONTENT_W, label.size().y + 2.0 * BTN_PAD),
                );
                note("connect_button", r);
                let hover = hit(r, action);
                let k = if hover { BTN_HOVER } else { 1.0 };
                draw::soft_shadow(p, r, BTN_RADIUS, 6.0, 18.0, colors::BTN_SHADOW);
                draw::gradient_button(
                    p,
                    r,
                    BTN_RADIUS,
                    brighten(colors::BTN_FROM, k),
                    brighten(colors::BTN_TO, k),
                );
                let at = pos2(r.center().x - label.size().x / 2.0, r.top() + BTN_PAD);
                p.galley(at, label.clone(), colors::BTN_LABEL);
            }
            Foot::Links(links) => {
                let right = left + CONTENT_W;
                for (i, (g, action)) in links.iter().enumerate() {
                    let x = if i == 0 { left } else { right - g.size().x };
                    let r = Rect::from_min_size(pos2(x, y), g.size());
                    note(if i == 0 { "link_0" } else { "link_1" }, r);
                    hit(r, action);
                    p.galley(r.min, g.clone(), colors::LINK);
                    let uy = y + baseline(g) + LINK_UNDERLINE_OFFSET;
                    p.line_segment(
                        [pos2(r.left(), uy), pos2(r.right(), uy)],
                        Stroke::new(1.0, colors::LINK),
                    );
                }
            }
        }
        Painted {
            actions,
            animating: self.spinner && !input.reduced_motion,
        }
    }
}

/// CSS `filter: brightness(k)` of an opaque color.
fn brighten(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (f32::from(v) * k).round().min(255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}
