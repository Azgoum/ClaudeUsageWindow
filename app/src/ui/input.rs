//! Pointer clicks with DOM semantics: a control is clicked only when the primary button
//! went down *and* came up inside it (pressing minimise and releasing on close does nothing).

use egui::{Event, PointerButton, Pos2, Rect};

/// Remembers where the primary button went down, across frames, until it comes up.
#[derive(Default)]
pub struct PressTracker {
    origin: Option<Pos2>,
}

/// This frame's pointer buttons, read from the raw events.
#[derive(Clone, Debug, Default)]
pub struct Clicks {
    /// Where the primary button went down this frame.
    pub press: Option<Pos2>,
    /// A primary release this frame, with the press it ends: (press origin, release).
    click: Option<(Pos2, Pos2)>,
    /// Where the secondary button came up this frame (context menu).
    pub secondary: Option<Pos2>,
    /// Areas covered by a popup: a click starting there belongs to the popup only.
    blocked: Vec<Rect>,
}

impl PressTracker {
    /// Reads this frame's button events.
    pub fn frame(&mut self, ctx: &egui::Context) -> Clicks {
        let mut clicks = Clicks::default();
        ctx.input(|i| {
            for e in &i.events {
                if let Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } = e
                {
                    match (button, pressed) {
                        (PointerButton::Primary, true) => {
                            self.origin = Some(*pos);
                            clicks.press = Some(*pos);
                        }
                        (PointerButton::Primary, false) => {
                            clicks.click = self.origin.take().map(|o| (o, *pos));
                        }
                        (PointerButton::Secondary, false) => clicks.secondary = Some(*pos),
                        _ => {}
                    }
                }
            }
        });
        clicks
    }

    /// Drops this frame's primary press and click: nothing else gets them, and the release
    /// that ends the press later is no click either.
    pub fn swallow(&mut self, clicks: &mut Clicks) {
        if clicks.press.is_some() {
            self.origin = None;
        }
        clicks.press = None;
        clicks.click = None;
    }
}

impl Clicks {
    /// Pressed and released inside `r`, not starting on a popup that covers it.
    pub fn clicked(&self, r: Rect) -> bool {
        self.click.is_some_and(|(from, to)| {
            r.contains(from) && r.contains(to) && !self.blocked.iter().any(|b| b.contains(from))
        })
    }

    /// From now on, presses inside `r` click nothing but what lies on top of it.
    pub fn block(&mut self, r: Rect) {
        self.blocked.push(r);
    }

    /// Whether `q` lies on a popup.
    pub fn is_blocked(&self, q: Pos2) -> bool {
        self.blocked.iter().any(|b| b.contains(q))
    }
}
