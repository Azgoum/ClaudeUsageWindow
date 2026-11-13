//! Non-Windows placeholder: keeps the pixels in memory and reports that
//! presenting is unsupported. A Mac port replaces this module.

use std::io;

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

    pub fn present(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "transparent window presentation is only implemented on Windows",
        ))
    }
}
