//! Platform integration. Windows code lives in `windows.rs`; a Mac port adds a
//! sibling module exposing the same surface.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use self::windows::LayeredSurface;
