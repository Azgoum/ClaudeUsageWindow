//! Per-pixel-alpha presentation: a `WS_EX_LAYERED` window fed by
//! `UpdateLayeredWindow` from a 32-bit premultiplied DIB section. No GPU, no
//! OS backdrop and no OS shadow; fully transparent pixels are click-through.

use std::{io, mem, ptr};

use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
        DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    },
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindow, GWL_EXSTYLE, STYLESTRUCT,
            ULW_ALPHA, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_LAYERED,
        },
    },
};

/// The raw HWND behind a winit window.
pub fn hwnd_of(window: &Window) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// Subclass id of [`keep_layered`] on the host window.
const SUBCLASS_ID: usize = 1;

/// The window's pixels, pushed to the screen with [`Surface::present`].
pub struct Surface {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    bits: *mut u32,
    width: usize,
    height: usize,
}

impl Surface {
    /// Makes `window` a layered window for good and prepares an empty surface.
    /// Must be called on the thread that owns `window` (the event loop thread).
    pub fn new(window: &Window) -> Self {
        let hwnd = hwnd_of(window).expect("a Win32 window handle") as HWND;
        // SAFETY: `hwnd` is a live window owned by this thread.
        unsafe {
            // winit rewrites GWL_EXSTYLE from its own flags whenever it changes a
            // style (show, minimize, always-on-top, ...), which would silently drop
            // WS_EX_LAYERED. The subclass keeps the bit in every style change.
            let subclassed = SetWindowSubclass(hwnd, Some(keep_layered), SUBCLASS_ID, 0);
            assert!(subclassed != 0, "SetWindowSubclass failed");
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
            // No visual styles for this window: on a frame change (always-on-top)
            // the theme gives a captioned window a region of its current size,
            // and once Windows has had one, the layered window's input shape
            // stays that size when the window grows (menu items below it get no
            // clicks), even after the region is removed.
            let empty = [0u16];
            windows_sys::Win32::UI::Controls::SetWindowTheme(hwnd, empty.as_ptr(), empty.as_ptr());
        }
        // SAFETY: a null DC asks for a memory DC compatible with the screen.
        let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
        assert!(!dc.is_null(), "CreateCompatibleDC failed");
        Self {
            hwnd,
            dc,
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

    /// Shows the current pixels, resizing the window to the buffer and, with
    /// `pos`, moving it there in the same step (no frame where the window has
    /// its new bounds but old pixels).
    pub fn present(&mut self, pos: Option<(i32, i32)>) -> io::Result<()> {
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
        self.drop_window_region();
        let dst = pos.map(|(x, y)| POINT { x, y });
        // SAFETY: all handles are live; a null destination point keeps the
        // window where it is.
        let ok = unsafe {
            GdiFlush();
            UpdateLayeredWindow(
                self.hwnd,
                ptr::null_mut(),
                dst.as_ref().map_or(ptr::null(), |p| p as *const POINT),
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
            let code = unsafe { GetLastError() };
            Err(io::Error::from_raw_os_error(code as i32))
        } else {
            Ok(())
        }
    }

    /// Safety net: a window region clips a layered window whatever
    /// `UpdateLayeredWindow` is given, so a popup that grows the window would be
    /// cut off below it. Turning theming off in `new` keeps Windows from adding
    /// one on a frame change; should one appear anyway, it is dropped and logged.
    fn drop_window_region(&self) {
        use windows_sys::Win32::Graphics::Gdi::{GetWindowRgnBox, SetWindowRgn, RGN_ERROR};
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: `bounds` is a valid out pointer; a null region removes it.
        unsafe {
            if GetWindowRgnBox(self.hwnd, &mut bounds) != RGN_ERROR {
                tracing::debug!(?bounds.right, ?bounds.bottom, "dropping a window region");
                SetWindowRgn(self.hwnd, ptr::null_mut(), 0);
            }
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

impl Drop for Surface {
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
    } else if msg == WM_NCDESTROY {
        // SAFETY: removes this subclass from the window it was installed on.
        unsafe { RemoveWindowSubclass(hwnd, Some(keep_layered), SUBCLASS_ID) };
    }
    // SAFETY: forwards the message unchanged to the next handler in the chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

// ---------------------------------------------------------------------------
// Single instance, monitors, window bounds, shell helpers.
// ---------------------------------------------------------------------------

use limits_core::Rect;
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, RECT},
    Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONEAREST,
    },
    System::Threading::CreateMutexW,
    UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            FindWindowW, GetWindowRect, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindow,
            SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SWP_NOACTIVATE, SWP_NOZORDER,
            SW_RESTORE, SW_SHOWNORMAL,
        },
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn rect_of(r: &RECT) -> Rect {
    Rect {
        x: r.left,
        y: r.top,
        w: r.right - r.left,
        h: r.bottom - r.top,
    }
}

/// Holds the single-instance mutex; the handle is closed on drop.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

pub fn single_instance(name: &str) -> Option<InstanceGuard> {
    let name = wide(name);
    // SAFETY: `name` is NUL-terminated and outlives the call.
    let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: reads the calling thread's last error right after the call.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: we own this duplicate handle.
        unsafe { CloseHandle(handle) };
        return None;
    }
    Some(InstanceGuard(handle))
}

/// Restores and focuses the top-level window of class `class` titled `title`.
pub fn focus_existing(class: &str, title: &str) -> bool {
    let (class, title) = (wide(class), wide(title));
    // SAFETY: both strings are NUL-terminated and outlive the call.
    unsafe {
        let hwnd = FindWindowW(class.as_ptr(), title.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
    true
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    out: LPARAM,
) -> i32 {
    // SAFETY: `out` is the `&mut Vec<Rect>` passed by `work_areas`.
    let out = unsafe { &mut *(out as *mut Vec<Rect>) };
    if let Some(r) = monitor_work_area(monitor) {
        out.push(r);
    }
    1
}

fn monitor_work_area(monitor: HMONITOR) -> Option<Rect> {
    // SAFETY: MONITORINFO is plain data; cbSize is set before the call.
    unsafe {
        let mut info: MONITORINFO = mem::zeroed();
        info.cbSize = mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| rect_of(&info.rcWork))
    }
}

pub fn work_areas() -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    // SAFETY: the callback only runs during this call and `out` outlives it.
    unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(collect_monitor),
            &mut out as *mut Vec<Rect> as LPARAM,
        );
    }
    out
}

/// The primary monitor's work area (the one at the origin).
pub fn primary_work_area() -> Option<Rect> {
    use windows_sys::Win32::Graphics::Gdi::MonitorFromPoint;
    use windows_sys::Win32::Graphics::Gdi::MONITOR_DEFAULTTOPRIMARY;
    // SAFETY: plain value arguments.
    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    monitor_work_area(monitor)
}

pub fn work_area_of(hwnd: isize) -> Option<Rect> {
    // SAFETY: an invalid HWND just yields the nearest (primary) monitor.
    let monitor = unsafe { MonitorFromWindow(hwnd as HWND, MONITOR_DEFAULTTONEAREST) };
    monitor_work_area(monitor)
}

pub fn window_rect(hwnd: isize) -> Option<Rect> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `r` is a valid out pointer.
    (unsafe { GetWindowRect(hwnd as HWND, &mut r) } != 0).then(|| rect_of(&r))
}

pub fn set_window_bounds(hwnd: isize, pos: (i32, i32), size: (i32, i32)) {
    // SAFETY: plain value arguments; failure leaves the window where it was.
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            ptr::null_mut(),
            pos.0,
            pos.1,
            size.0,
            size.1,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn spurious_leave(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        WindowsAndMessaging::{GetCursorPos, WindowFromPoint},
    };
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` is a valid out pointer; WindowFromPoint honors the layered
    // window's per-pixel hit testing (transparent pixels are not the window).
    let over = unsafe { GetCursorPos(&mut at) != 0 && WindowFromPoint(at) == hwnd as HWND };
    if over {
        let mut track = TRACKMOUSEEVENT {
            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd as HWND,
            dwHoverTime: 0,
        };
        // SAFETY: `track` is fully initialized and `hwnd` belongs to this thread.
        unsafe { TrackMouseEvent(&mut track) };
    }
    over
}

pub fn is_minimized(hwnd: isize) -> bool {
    // SAFETY: plain value argument.
    unsafe { IsIconic(hwnd as HWND) != 0 }
}

pub fn open_url(url: &str) -> io::Result<()> {
    let verb = wide("open");
    let url = wide(url);
    // SAFETY: both strings are NUL-terminated; null parameters are allowed.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW reports success with a value greater than 32.
    if result as usize > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "ShellExecuteW failed with code {}",
            result as usize
        )))
    }
}

pub fn reduced_motion() -> bool {
    let mut enabled: i32 = 1;
    // SAFETY: for this action pvParam points at a BOOL.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            &mut enabled as *mut i32 as *mut _,
            0,
        )
    };
    ok != 0 && enabled == 0
}
