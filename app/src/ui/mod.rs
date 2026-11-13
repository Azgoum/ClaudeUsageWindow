//! Widget UI: pure logic (text, formatting, colors, geometry, fixtures) and shared interfaces.

pub mod colors;
pub mod draw;
pub mod fonts;
pub mod format;
pub mod i18n;
pub mod menu;
pub mod ring;
pub mod room;

#[cfg(test)]
pub mod fixtures;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub lang: Lang,
    pub always_on_top: bool,
    pub start_with_windows: bool,
}

pub struct FrameInput<'a> {
    pub state: &'a limits_core::AppState,
    pub prefs: &'a Prefs,
    /// Wall clock (countdowns, "updated X ago").
    pub now: chrono::DateTime<chrono::Utc>,
    /// Local offset for "14:00" / "Fri 23:59".
    pub tz: chrono::FixedOffset,
    /// Monotonic animation clock, seconds.
    pub t: f64,
    /// Windows "Show animations" is off.
    pub reduced_motion: bool,
    /// Points the widget sits below the window top (window grown upward).
    pub offset_y: f32,
    /// The window has focus (the menu closes on blur).
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Send(limits_core::Command),
    SetLang(Lang),
    SetAlwaysOnTop(bool),
    SetStartWithWindows(bool),
    OpenUrl(&'static str),
    Minimize,
    StartDrag,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Repaint {
    Idle,
    After(std::time::Duration),
    Now,
}

pub struct FrameOutput {
    pub actions: Vec<Action>,
    /// Widget box incl. border, points.
    pub size: egui::Vec2,
    /// Popup room below the widget, points (0 = rest).
    pub extra: f32,
    pub repaint: Repaint,
}
