//! PROTOTYPE — macOS presenter: CPU pixmap -> CGImage -> CALayer.contents.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSView};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_quartz_core::{CALayer, CATransaction};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use resvg::tiny_skia::Pixmap;
use winit::window::Window;

pub struct Presenter {
    layer: Retained<CALayer>,
    space: CFRetained<CGColorSpace>,
}

fn ns_view(window: &Window) -> Retained<NSView> {
    let RawWindowHandle::AppKit(h) = window.window_handle().unwrap().as_raw() else { unreachable!() };
    unsafe { Retained::retain(h.ns_view.as_ptr().cast::<NSView>()) }.unwrap()
}

impl Presenter {
    pub fn new(window: &Window) -> Self {
        let view = ns_view(window);
        view.setWantsLayer(true);
        let root = view.layer().unwrap();
        let layer = CALayer::new();
        layer.setFrame(view.bounds());
        layer.setContentsScale(window.scale_factor());
        root.addSublayer(&layer);
        Self { layer, space: CGColorSpace::new_device_rgb().unwrap() }
    }

    pub fn present(&mut self, window: &Window, pixmap: &Pixmap) {
        let data = CFData::from_bytes(pixmap.data());
        let provider = CGDataProvider::with_cf_data(Some(&data)).unwrap();
        let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
        let image = unsafe {
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&self.space),
                CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0),
                Some(&provider),
                std::ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .unwrap();
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        self.layer.setContentsScale(window.scale_factor());
        let obj: &AnyObject = unsafe { &*(CFRetained::as_ptr(&image).as_ptr() as *const AnyObject) };
        unsafe { self.layer.setContents(Some(obj)) };
        CATransaction::commit();
    }
}

/// Global cursor position converted into this window's physical pixel coords.
pub fn cursor_in_window_px(window: &Window) -> (f64, f64) {
    let view = ns_view(window);
    let ns_window = view.window().unwrap();
    let frame = ns_window.frame();
    let m = NSEvent::mouseLocation();
    let s = ns_window.backingScaleFactor();
    ((m.x - frame.origin.x) * s, (frame.origin.y + frame.size.height - m.y) * s)
}
