//! Platform integration. Windows code lives in `windows.rs`; other targets get
//! `stub.rs`, so the crate builds everywhere and a Mac port replaces the stub
//! with its own module exposing the same surface.

pub mod autostart;
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

use limits_core::Rect;

pub use imp::InstanceGuard;

/// Claims the named single-instance mutex; `None` when another process holds it.
pub fn single_instance(name: &str) -> Option<InstanceGuard> {
    imp::single_instance(name)
}

/// Restores and focuses the window titled `title`; false when none exists.
pub fn focus_existing(title: &str) -> bool {
    imp::focus_existing(title)
}

/// Work areas of every monitor, in physical pixels.
pub fn work_areas() -> Vec<Rect> {
    imp::work_areas()
}

pub fn primary_work_area() -> Option<Rect> {
    imp::primary_work_area()
}

/// Work area of the monitor nearest to `hwnd` (a raw HWND value).
pub fn work_area_of(hwnd: isize) -> Option<Rect> {
    imp::work_area_of(hwnd)
}

/// Outer rectangle of `hwnd` in physical pixels.
pub fn window_rect(hwnd: isize) -> Option<Rect> {
    imp::window_rect(hwnd)
}

/// Moves and resizes `hwnd` without activating it or changing its z-order.
pub fn set_window_bounds(hwnd: isize, pos: (i32, i32), size: (i32, i32)) {
    imp::set_window_bounds(hwnd, pos, size)
}

pub fn is_minimized(hwnd: isize) -> bool {
    imp::is_minimized(hwnd)
}

/// Opens `url` with the default handler.
pub fn open_url(url: &str) -> std::io::Result<()> {
    imp::open_url(url)
}

/// True when the user turned off system animations.
pub fn reduced_motion() -> bool {
    imp::reduced_motion()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn second_instance_is_refused() {
        let name = format!(r"Local\limitrings-test-{}", std::process::id());
        let a = single_instance(&name);
        assert!(a.is_some());
        assert!(single_instance(&name).is_none());
        drop(a);
        assert!(single_instance(&name).is_some());
    }

    #[test]
    fn primary_work_area_exists() {
        assert!(primary_work_area().is_some_and(|r| r.w > 0 && r.h > 0));
        assert!(!work_areas().is_empty());
    }
}
