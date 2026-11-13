//! Widget UI. Task 5 onward adds the rest; for now only the shared preference types.

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
