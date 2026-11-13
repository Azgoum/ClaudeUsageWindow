//! Right-click menu (port of `WidgetMenu.tsx` and `index.css` `.menu`): opens at the cursor,
//! closes after any item, on Escape, on a press outside it and when the window loses focus.
//!
//! `.menu { min-width: 200px; padding: 5px; border-radius: 10px; font: 12px Inter }` (border
//! box), `.it { padding: 6px 10px 6px 26px; border-radius: 6px }`, `.hd { padding: 6px 10px
//! 2px; font-size: 10px; letter-spacing: .06em }`, `.sep { height: 1px; margin: 4px 2px }`.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Color32, CornerRadius, CursorIcon, Galley, Key, Painter, Pos2, Rect, Stroke,
};
use limits_core::{AppState, Command};

use super::i18n::t;
use super::input::{Clicks, PressTracker};
use super::menu::connected_items;
use super::room::{menu_room, PAD};
use super::{colors, draw, fonts, Action, FrameInput, Prefs};

const BORDER: f32 = 1.0;
const PAD_BOX: f32 = 5.0;
const MIN_W: f32 = 200.0;
const RADIUS: f32 = 10.0;
const ITEM_SIZE: f32 = 12.0;
const ITEM_PAD_Y: f32 = 6.0;
const ITEM_PAD_LEFT: f32 = 26.0;
const ITEM_PAD_RIGHT: f32 = 10.0;
const ITEM_RADIUS: u8 = 6;
const CHECK_X: f32 = 9.0;
const HEAD_SIZE: f32 = 10.0;
const HEAD_PAD_TOP: f32 = 6.0;
const HEAD_PAD_BOTTOM: f32 = 2.0;
const HEAD_PAD_X: f32 = 10.0;
const HEAD_SPACING: f32 = 0.06 * HEAD_SIZE;
const SEP_MARGIN_Y: f32 = 4.0;
const SEP_MARGIN_X: f32 = 2.0;
/// `box-shadow: 0 16px 40px rgba(0,0,0,.5)`.
const SHADOW_Y: f32 = 16.0;
const SHADOW_BLUR: f32 = 40.0;
/// CSS `line-height: normal` of Inter: (ascender + descender) / em = (1984 + 494) / 2048.
const NORMAL_LINE: f32 = 1.21;

/// One row of the menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Item {
        label: String,
        check: bool,
        danger: bool,
        action: Action,
    },
    Header(String),
    Sep,
}

fn item(label: &str, check: bool, action: Action) -> Entry {
    Entry::Item {
        label: label.to_owned(),
        check,
        danger: false,
        action,
    }
}

/// The rows for `state` and `prefs`, in order.
pub fn entries(state: &AppState, prefs: &Prefs) -> Vec<Entry> {
    let l = t(prefs.lang);
    let mut v = vec![];
    let connected = connected_items(&state.conn);
    if connected {
        v.push(item(l.m_refresh, false, Action::Send(Command::Refresh)));
        v.push(Entry::Sep);
        if state.orgs.len() > 1 {
            v.push(Entry::Header(l.m_org.to_uppercase()));
            let current = state.org.as_ref().map(|o| o.uuid.as_str());
            for o in &state.orgs {
                let action = Action::Send(Command::SelectOrg(o.uuid.clone()));
                v.push(item(&o.name, current == Some(o.uuid.as_str()), action));
            }
            v.push(Entry::Sep);
        }
    }
    v.push(item(
        l.m_top,
        prefs.always_on_top,
        Action::SetAlwaysOnTop(!prefs.always_on_top),
    ));
    v.push(item(
        l.m_boot,
        prefs.start_with_windows,
        Action::SetStartWithWindows(!prefs.start_with_windows),
    ));
    v.push(Entry::Sep);
    if connected {
        v.push(item(l.m_disc, false, Action::Send(Command::Disconnect)));
    }
    v.push(Entry::Item {
        label: l.m_quit.to_owned(),
        check: false,
        danger: true,
        action: Action::Quit,
    });
    v
}

enum RowKind {
    Item {
        label: Arc<Galley>,
        check: Option<Arc<Galley>>,
        action: Action,
        #[cfg_attr(not(test), expect(dead_code))]
        text: String,
    },
    Header(Arc<Galley>),
    Sep,
}

struct Row {
    /// Offset from the box top-left and size.
    rect: Rect,
    kind: RowKind,
}

/// A laid-out menu, rows relative to its top-left corner.
pub struct Layout {
    size: egui::Vec2,
    rows: Vec<Row>,
}

fn galley(p: &Painter, text: &str, size: f32, color: Color32, spacing: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: fonts::regular(size),
            color,
            line_height: Some(size * NORMAL_LINE),
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    job.wrap.max_width = f32::INFINITY;
    p.layout_job(job)
}

impl Layout {
    pub fn new(p: &Painter, entries: Vec<Entry>) -> Layout {
        let item_h = 2.0 * ITEM_PAD_Y + ITEM_SIZE * NORMAL_LINE;
        let head_h = HEAD_PAD_TOP + HEAD_PAD_BOTTOM + HEAD_SIZE * NORMAL_LINE;
        let sep_h = 2.0 * SEP_MARGIN_Y + 1.0;
        let mut rows = vec![];
        let mut y = 0.0;
        let mut inner_w: f32 = 0.0;
        for e in entries {
            let (h, kind) = match e {
                Entry::Item {
                    label,
                    check,
                    danger,
                    action,
                } => {
                    let color = if danger {
                        colors::ERR_TEXT
                    } else {
                        colors::INK
                    };
                    let g = galley(p, &label, ITEM_SIZE, color, 0.0);
                    inner_w = inner_w.max(ITEM_PAD_LEFT + g.size().x + ITEM_PAD_RIGHT);
                    let check = check.then(|| galley(p, "✓", ITEM_SIZE, colors::LINK, 0.0));
                    let kind = RowKind::Item {
                        label: g,
                        check,
                        action,
                        text: label,
                    };
                    (item_h, kind)
                }
                Entry::Header(text) => {
                    let g = galley(p, &text, HEAD_SIZE, colors::IDLE, HEAD_SPACING);
                    inner_w = inner_w.max(2.0 * HEAD_PAD_X + g.size().x);
                    (head_h, RowKind::Header(g))
                }
                Entry::Sep => (sep_h, RowKind::Sep),
            };
            rows.push(Row {
                rect: Rect::from_min_size(pos2(0.0, y), vec2(0.0, h)),
                kind,
            });
            y += h;
        }
        let edge = BORDER + PAD_BOX;
        let w = (inner_w + 2.0 * edge).max(MIN_W);
        for r in &mut rows {
            r.rect = Rect::from_min_size(
                pos2(edge, edge + r.rect.top()),
                vec2(w - 2.0 * edge, r.rect.height()),
            );
        }
        Layout {
            size: vec2(w, y + 2.0 * edge),
            rows,
        }
    }

    fn rect_at(&self, min: Pos2) -> Rect {
        Rect::from_min_size(min, self.size)
    }

    /// Paints the menu with its top-left corner at `min`; `pointer` highlights its item.
    pub fn paint(&self, ctx: &egui::Context, p: &Painter, min: Pos2, pointer: Option<Pos2>) {
        let rect = self.rect_at(min);
        draw::soft_shadow(p, rect, RADIUS, SHADOW_Y, SHADOW_BLUR, colors::MENU_SHADOW);
        p.rect(
            rect,
            CornerRadius::same(RADIUS as u8),
            colors::MENU_BG,
            Stroke::new(BORDER, colors::MENU_BORDER),
            egui::StrokeKind::Inside,
        );
        for row in &self.rows {
            let r = row.rect.translate(min.to_vec2());
            match &row.kind {
                RowKind::Item { label, check, .. } => {
                    if pointer.is_some_and(|q| r.contains(q)) {
                        ctx.set_cursor_icon(CursorIcon::PointingHand);
                        p.rect_filled(r, CornerRadius::same(ITEM_RADIUS), colors::MENU_HOVER);
                    }
                    let y = r.top() + ITEM_PAD_Y;
                    if let Some(c) = check {
                        p.galley(pos2(r.left() + CHECK_X, y), c.clone(), colors::LINK);
                    }
                    p.galley(
                        pos2(r.left() + ITEM_PAD_LEFT, y),
                        label.clone(),
                        colors::INK,
                    );
                }
                RowKind::Header(g) => {
                    let at = pos2(r.left() + HEAD_PAD_X, r.top() + HEAD_PAD_TOP);
                    p.galley(at, g.clone(), colors::IDLE);
                }
                RowKind::Sep => {
                    let y = r.top() + SEP_MARGIN_Y;
                    let line = Rect::from_min_max(
                        pos2(r.left() + SEP_MARGIN_X, y),
                        pos2(r.right() - SEP_MARGIN_X, y + 1.0),
                    );
                    p.rect_filled(line, 0, colors::MENU_SEP);
                }
            }
        }
    }
}

/// The open menu, this frame.
pub struct Shown {
    pub layout: Layout,
    /// Top-left corner, window points.
    pub min: Pos2,
    /// Popup room below the widget it needs.
    pub room: f32,
}

impl Shown {
    pub fn rect(&self) -> Rect {
        self.layout.rect_at(self.min)
    }

    /// Item labels and their window rects (tests).
    #[cfg(test)]
    pub fn items(&self) -> Vec<(String, Rect)> {
        self.layout
            .rows
            .iter()
            .filter_map(|r| match &r.kind {
                RowKind::Item { text, .. } => {
                    Some((text.clone(), r.rect.translate(self.min.to_vec2())))
                }
                _ => None,
            })
            .collect()
    }
}

#[derive(Default)]
pub struct Menu {
    /// Where it opened, relative to the widget's top-left corner.
    anchor: Option<egui::Vec2>,
    was_focused: bool,
}

impl Menu {
    pub fn is_open(&self) -> bool {
        self.anchor.is_some()
    }

    pub fn close(&mut self) {
        self.anchor = None;
    }

    /// Handles this frame's input for the widget box `widget` and lays the menu out.
    /// A press outside the open menu closes it and is swallowed; an item click acts and
    /// closes it. While open, its box is blocked for every other control.
    #[expect(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        p: &Painter,
        widget: Rect,
        input: &FrameInput,
        presses: &mut PressTracker,
        clicks: &mut Clicks,
        actions: &mut Vec<Action>,
    ) -> Option<Shown> {
        if self.was_focused && !input.focused {
            self.close();
        }
        self.was_focused = input.focused;
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.close();
        }
        let lay = |anchor: egui::Vec2| {
            let layout = Layout::new(p, entries(input.state, input.prefs));
            let window_w = widget.width();
            let x = (widget.left() + anchor.x)
                .min(widget.left() + window_w - PAD - layout.size.x)
                .max(widget.left() + PAD);
            let min = pos2(x, widget.top() + anchor.y);
            let room = menu_room(anchor.y, layout.size.y, widget.height());
            Shown { layout, min, room }
        };
        if let Some(anchor) = self.anchor {
            let shown = lay(anchor);
            let rect = shown.rect();
            if clicks.press.is_some_and(|q| !rect.contains(q)) {
                presses.swallow(clicks);
                self.close();
            } else {
                for row in &shown.layout.rows {
                    if let RowKind::Item { action, .. } = &row.kind {
                        if clicks.clicked(row.rect.translate(shown.min.to_vec2())) {
                            actions.push(action.clone());
                            self.close();
                        }
                    }
                }
            }
            // The click that picked an item belongs to the menu, even as it closes.
            clicks.block(rect);
        }
        if let Some(q) = clicks.secondary {
            let on_menu = self.anchor.is_some_and(|a| lay(a).rect().contains(q));
            if widget.contains(q) && !on_menu {
                self.anchor = Some(q - widget.min);
            }
        }
        let shown = self.anchor.map(lay)?;
        clicks.block(shown.rect());
        Some(shown)
    }
}

#[cfg(test)]
mod tests {
    use egui::{pos2, Pos2};
    use limits_core::Command;

    use super::*;
    use crate::ui::widget::snapshot_tests::{click_at, fit, fixture, render, H};
    use crate::ui::{FrameOutput, Lang};

    /// Where the menus open: the glass between the ring columns (no tooltip once closed).
    const AT: Pos2 = pos2(122.0, 60.0);

    fn open_menu(kind: &str, lang: Lang, at: Pos2) -> H {
        let mut h = render(kind, lang, 1.0);
        click_at(&mut h, at, egui::PointerButton::Secondary);
        h.run();
        fit(&mut h);
        h
    }

    fn out(mut h: H) -> FrameOutput {
        h.state_mut().out.take().expect("shown")
    }

    fn menu_labels(kind: &str, lang: Lang) -> Vec<String> {
        let f = fixture(kind, lang);
        entries(&f.state, &f.prefs)
            .into_iter()
            .filter_map(|e| match e {
                Entry::Item { label, .. } => Some(label),
                _ => None,
            })
            .collect()
    }

    fn click_menu_item(kind: &str, label: &str) -> Vec<Action> {
        let mut h = open_menu(kind, Lang::En, AT);
        let items = h.state().widget.debug_menu_items().to_vec();
        let (_, r) = items.iter().find(|(l, _)| l == label).expect("item");
        click_at(&mut h, r.center(), egui::PointerButton::Primary);
        h.state().out.as_ref().unwrap().actions.clone()
    }

    fn open_menu_then_blur(kind: &str) -> FrameOutput {
        let mut h = open_menu(kind, Lang::En, AT);
        assert!(h.state().out.as_ref().unwrap().extra > 0.0);
        h.state_mut().focused = false;
        h.run();
        out(h)
    }

    /// Opens the menu on the session ring, then rests the pointer there.
    fn tooltip_visible_with_menu_open(kind: &str) -> bool {
        let ring =
            render(kind, Lang::En, 1.0).state().widget.debug_rects()["ring_session"].center();
        let mut h = open_menu(kind, Lang::En, ring);
        h.input_mut().events.push(egui::Event::PointerMoved(ring));
        h.run();
        let rects = h.state().widget.debug_rects();
        assert!(rects.contains_key("menu"));
        rects.contains_key("tip_session") || rects.contains_key("tip_weekly")
    }

    #[test]
    fn menu_items_for_disconnected() {
        assert_eq!(
            menu_labels("disconnected", Lang::En),
            ["Always on top", "Start with Windows", "Quit"]
        );
    }

    #[test]
    fn menu_items_for_two_orgs() {
        assert_eq!(
            menu_labels("twoOrgs", Lang::En),
            [
                "Refresh now",
                "Jane's Organization",
                "Acme Studio",
                "Always on top",
                "Start with Windows",
                "Disconnect",
                "Quit"
            ]
        );
    }

    #[test]
    fn select_org_sends_uuid() {
        assert_eq!(
            click_menu_item("twoOrgs", "Acme Studio"),
            vec![Action::Send(Command::SelectOrg("org-team".into()))]
        );
    }

    #[test]
    fn items_send_their_action() {
        assert_eq!(
            click_menu_item("connected", "Refresh now"),
            vec![Action::Send(Command::Refresh)]
        );
        // Fixture prefs: always on top, not started with Windows.
        assert_eq!(
            click_menu_item("connected", "Always on top"),
            vec![Action::SetAlwaysOnTop(false)]
        );
        assert_eq!(
            click_menu_item("connected", "Start with Windows"),
            vec![Action::SetStartWithWindows(true)]
        );
        assert_eq!(
            click_menu_item("connected", "Disconnect"),
            vec![Action::Send(Command::Disconnect)]
        );
        assert_eq!(click_menu_item("disconnected", "Quit"), vec![Action::Quit]);
    }

    #[test]
    fn item_click_closes_the_menu() {
        // "Disconnect" lies below the rings: no tooltip opens under the pointer afterwards.
        let mut h = open_menu("connected", Lang::En, AT);
        let items = h.state().widget.debug_menu_items().to_vec();
        let (_, r) = items.iter().find(|(l, _)| l == "Disconnect").unwrap();
        click_at(&mut h, r.center(), egui::PointerButton::Primary);
        assert!(!h.state().widget.debug_rects().contains_key("menu"));
        assert_eq!(h.state().out.as_ref().unwrap().extra, 0.0);
    }

    #[test]
    fn menu_closes_on_blur() {
        assert_eq!(open_menu_then_blur("connected").extra, 0.0);
    }

    #[test]
    fn unfocused_window_still_opens_the_menu() {
        // Only losing focus closes it, not lacking it.
        let mut h = render("connected", Lang::En, 1.0);
        h.state_mut().focused = false;
        click_at(&mut h, AT, egui::PointerButton::Secondary);
        assert!(h.state().out.as_ref().unwrap().extra > 0.0);
    }

    #[test]
    fn menu_closes_on_escape() {
        let mut h = open_menu("connected", Lang::En, AT);
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(out(h).extra, 0.0);
    }

    #[test]
    fn press_outside_closes_without_drag_or_click() {
        // On the glass (would drag), then on close (would quit): each only closes the menu.
        for name in ["glass", "ctl_close"] {
            let mut h = open_menu("connected", Lang::En, AT);
            let at = match name {
                "glass" => pos2(122.0, 15.0),
                _ => h.state().widget.debug_rects()[name].center(),
            };
            click_at(&mut h, at, egui::PointerButton::Primary);
            let o = h.state().out.as_ref().unwrap();
            assert!(o.actions.is_empty(), "{name}: {:?}", o.actions);
            assert_eq!(o.extra, 0.0, "{name}");
        }
    }

    #[test]
    fn menu_over_a_control_takes_the_click() {
        // Opened at the top right: shifted left, over minimise and close.
        let mut h = open_menu("connected", Lang::En, pos2(230.0, 10.0));
        let menu = h.state().widget.debug_rects()["menu"];
        let close = h.state().widget.debug_rects()["ctl_close"];
        assert!(menu.intersects(close));
        assert!(menu.right() <= 238.0 && menu.left() >= 6.0);
        let item = h.state().widget.debug_menu_items()[0].1;
        assert!(item.contains(pos2(close.center().x, item.center().y)));
        let at = pos2(close.center().x, item.center().y);
        click_at(&mut h, at, egui::PointerButton::Primary);
        assert_eq!(
            h.state().out.as_ref().unwrap().actions,
            vec![Action::Send(Command::Refresh)]
        );
    }

    #[test]
    fn no_tooltip_while_menu_open() {
        assert!(!tooltip_visible_with_menu_open("connected"));
    }

    #[test]
    fn menu_room_matches_its_height() {
        let h = open_menu("connected", Lang::En, AT);
        let menu = h.state().widget.debug_rects()["menu"];
        let o = h.state().out.as_ref().unwrap();
        assert_eq!(o.extra, (menu.bottom() + 6.0 - o.size.y).ceil());
    }

    #[test]
    fn menu_connected_en() {
        open_menu("connected", Lang::En, AT).snapshot("menu_connected_en");
    }
    #[test]
    fn menu_two_orgs_en() {
        open_menu("twoOrgs", Lang::En, AT).snapshot("menu_two_orgs_en");
    }
    #[test]
    fn menu_two_orgs_fr() {
        open_menu("twoOrgs", Lang::Fr, AT).snapshot("menu_two_orgs_fr");
    }
    #[test]
    fn menu_disconnected_en() {
        open_menu("disconnected", Lang::En, AT).snapshot("menu_disconnected_en");
    }
}
