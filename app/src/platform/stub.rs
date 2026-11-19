//! Non-Windows placeholder: keeps the pixels in memory and reports that
//! presenting is unsupported. A Mac port replaces this module.

use std::io;

use limits_core::Rect;
use winit::window::Window;

pub struct Surface {
    pixels: Vec<u32>,
}

impl Surface {
    pub fn new(_window: &Window) -> Self {
        Self { pixels: Vec::new() }
    }

    pub fn pixels(&mut self, width: usize, height: usize) -> &mut [u32] {
        self.pixels.resize(width.max(1) * height.max(1), 0);
        &mut self.pixels
    }

    pub fn present(&mut self, _pos: Option<(i32, i32)>) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "transparent window presentation is only implemented on Windows",
        ))
    }
}

pub struct InstanceGuard;

pub fn single_instance(_name: &str) -> Option<InstanceGuard> {
    Some(InstanceGuard)
}

pub fn hwnd_of(_window: &Window) -> Option<isize> {
    None
}

pub fn focus_existing(_class: &str, _title: &str) -> bool {
    false
}

pub fn work_areas() -> Vec<Rect> {
    Vec::new()
}

pub fn primary_work_area() -> Option<Rect> {
    None
}

pub fn work_area_of(_hwnd: isize) -> Option<Rect> {
    None
}

pub fn window_rect(_hwnd: isize) -> Option<Rect> {
    None
}

pub fn set_window_bounds(_hwnd: isize, _pos: (i32, i32), _size: (i32, i32)) {}

pub fn spurious_leave(_hwnd: isize) -> bool {
    false
}

pub fn is_minimized(_hwnd: isize) -> bool {
    false
}

pub fn open_url(_url: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening URLs is only implemented on Windows",
    ))
}

pub fn reduced_motion() -> bool {
    false
}
