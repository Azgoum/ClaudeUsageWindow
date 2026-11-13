//! Per-pixel-alpha presentation: a `WS_EX_LAYERED` window fed by
//! `UpdateLayeredWindow` from a 32-bit premultiplied DIB section. No GPU, no
//! OS backdrop and no OS shadow; fully transparent pixels are click-through.

use std::{mem, ptr};

use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
        DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    },
    UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindow, GWL_EXSTYLE, STYLESTRUCT,
            ULW_ALPHA, WM_STYLECHANGING, WS_EX_LAYERED,
        },
    },
};

/// The window's pixels, pushed to the screen with [`LayeredSurface::present`].
pub struct LayeredSurface {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u32,
    width: usize,
    height: usize,
}

impl LayeredSurface {
    /// Makes `hwnd` a layered window for good and prepares an empty surface.
    ///
    /// `hwnd` must be a live top-level window owned by the calling thread.
    pub fn new(hwnd: isize) -> Self {
        let hwnd = hwnd as HWND;
        // SAFETY: `hwnd` is a live window of this thread (caller contract).
        unsafe {
            // winit rewrites GWL_EXSTYLE from its own flags whenever it changes a
            // style (show, minimize, always-on-top, ...), which would silently drop
            // WS_EX_LAYERED. The subclass keeps the bit in every style change.
            SetWindowSubclass(hwnd, Some(keep_layered), 1, 0);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        }
        Self {
            hwnd,
            // SAFETY: a null DC asks for a memory DC compatible with the screen.
            dc: unsafe { CreateCompatibleDC(ptr::null_mut()) },
            bitmap: ptr::null_mut(),
            old_bitmap: ptr::null_mut(),
            bits: ptr::null_mut(),
            width: 0,
            height: 0,
        }
    }

    /// The pixel buffer for a `width` x `height` frame: top-down rows of
    /// premultiplied `0xAARRGGBB`. Reallocated only when the size changes.
    pub fn pixels(&mut self, width: usize, height: usize) -> &mut [u32] {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) != (self.width, self.height) {
            self.release_bitmap();
            self.allocate(width, height);
        }
        // SAFETY: `bits` points at the `width * height` u32 DIB owned by `self`.
        unsafe { std::slice::from_raw_parts_mut(self.bits, width * height) }
    }

    /// Shows the current pixels, resizing the window to the buffer.
    /// Returns the Win32 error code if `UpdateLayeredWindow` fails.
    pub fn present(&mut self) -> Result<(), u32> {
        let size = SIZE {
            cx: self.width as i32,
            cy: self.height as i32,
        };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: all handles are live; a null destination point keeps the
        // window where it is.
        let ok = unsafe {
            GdiFlush();
            UpdateLayeredWindow(
                self.hwnd,
                ptr::null_mut(),
                ptr::null(),
                &size,
                self.dc,
                &origin,
                0,
                &blend,
                ULW_ALPHA,
            )
        };
        if ok == 0 {
            // SAFETY: reads the calling thread's last-error value.
            Err(unsafe { GetLastError() })
        } else {
            Ok(())
        }
    }

    fn allocate(&mut self, width: usize, height: usize) {
        // SAFETY: plain GDI allocation; a negative height makes the DIB top-down.
        unsafe {
            let mut info: BITMAPINFO = mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..mem::zeroed()
            };
            let mut bits = ptr::null_mut();
            let bitmap = CreateDIBSection(
                self.dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                ptr::null_mut(),
                0,
            );
            assert!(!bitmap.is_null(), "CreateDIBSection failed");
            self.bitmap = bitmap;
            self.old_bitmap = SelectObject(self.dc, bitmap);
            self.bits = bits.cast();
        }
        self.width = width;
        self.height = height;
    }

    fn release_bitmap(&mut self) {
        if !self.bitmap.is_null() {
            // SAFETY: puts the DC's original bitmap back before deleting ours.
            unsafe {
                SelectObject(self.dc, self.old_bitmap);
                DeleteObject(self.bitmap);
            }
            self.bitmap = ptr::null_mut();
        }
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        self.release_bitmap();
        // SAFETY: the DC was created by `CreateCompatibleDC` in `new`.
        unsafe { DeleteDC(self.dc) };
    }
}

unsafe extern "system" fn keep_layered(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_STYLECHANGING && wparam as i32 == GWL_EXSTYLE {
        // SAFETY: for WM_STYLECHANGING, lparam points at a writable STYLESTRUCT.
        let styles = unsafe { &mut *(lparam as *mut STYLESTRUCT) };
        styles.styleNew |= WS_EX_LAYERED;
    }
    // SAFETY: forwards the message unchanged to the next handler in the chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
