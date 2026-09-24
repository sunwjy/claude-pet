//! PROTOTYPE — Windows presenter: WS_EX_LAYERED + UpdateLayeredWindow.
//! Alpha-0 pixels of a per-pixel-alpha layered window are click-through
//! natively, so Mode::Native should already be pixel-exact here.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use resvg::tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteObject, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindow, GWL_EXSTYLE, ULW_ALPHA,
    WS_EX_LAYERED,
};
use winit::window::Window;

pub struct Presenter {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    size: (u32, u32),
}

impl Presenter {
    pub fn new(window: &Window) -> Self {
        let RawWindowHandle::Win32(h) = window.window_handle().unwrap().as_raw() else { unreachable!() };
        let hwnd = h.hwnd.get() as HWND;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
            Self {
                hwnd,
                dc: CreateCompatibleDC(std::ptr::null_mut()),
                bitmap: std::ptr::null_mut(),
                bits: std::ptr::null_mut(),
                size: (0, 0),
            }
        }
    }

    fn ensure_bitmap(&mut self, w: u32, h: u32) {
        if self.size == (w, h) && !self.bitmap.is_null() {
            return;
        }
        unsafe {
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits = std::ptr::null_mut();
            self.bitmap = CreateDIBSection(self.dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            self.bits = bits.cast();
            SelectObject(self.dc, self.bitmap);
        }
        self.size = (w, h);
    }

    pub fn present(&mut self, _window: &Window, pixmap: &Pixmap) {
        let (w, h) = (pixmap.width(), pixmap.height());
        self.ensure_bitmap(w, h);
        // tiny-skia is premultiplied RGBA; DIB wants premultiplied BGRA.
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, (w * h * 4) as usize) };
        for (d, s) in dst.chunks_exact_mut(4).zip(pixmap.data().chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let size = SIZE { cx: w as i32, cy: h as i32 };
        let src = POINT { x: 0, y: 0 };
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
                self.dc,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            );
        }
    }
}
