//! Platform integration. Windows code lives in `windows.rs`; other targets get
//! `stub.rs`, so the crate builds everywhere and a Mac port replaces the stub
//! with its own module exposing the same surface.

#[cfg(not(windows))]
mod stub;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use self::stub as imp;
#[cfg(windows)]
use self::windows as imp;

/// A window's pixel surface: the host renders into `Surface::pixels`
/// (top-down rows of premultiplied `0xAARRGGBB`) and shows them with
/// `Surface::present`, which also sizes the window to the buffer.
pub use imp::Surface;

/// Prepares `window` for per-pixel-alpha presentation and returns its surface.
/// Call it on the event loop thread, before the window is first shown.
pub fn surface_for(window: &winit::window::Window) -> Surface {
    Surface::new(window)
}
