//! PROTOTYPE — X11 presenter: PutImage into the ARGB window winit created, plus
//! an XShape *input* region rebuilt from the alpha mask on every frame, so
//! Mode::Native should be pixel-exact here. Needs a compositor for alpha.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use resvg::tiny_skia::Pixmap;
use winit::window::Window;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ClipOrdering, ConnectionExt as _, CreateGCAux, ImageFormat, Rectangle};
use x11rb::rust_connection::RustConnection;

pub struct Presenter {
    conn: RustConnection,
    win: u32,
    gc: u32,
    last_mask: Vec<(i16, i16, u16)>,
}

impl Presenter {
    pub fn new(window: &Window) -> Self {
        let win = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            other => panic!("not an X11 window (Wayland? run under XWayland): {other:?}"),
        };
        let (conn, _) = x11rb::connect(None).unwrap();
        let gc = conn.generate_id().unwrap();
        conn.create_gc(gc, win, &CreateGCAux::new()).unwrap();
        conn.flush().unwrap();
        Self { conn, win, gc, last_mask: Vec::new() }
    }

    pub fn present(&mut self, _window: &Window, pixmap: &Pixmap) {
        let (w, h) = (pixmap.width(), pixmap.height());
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        let mut rects = Vec::new();
        for (y, row) in pixmap.data().chunks_exact((w * 4) as usize).enumerate() {
            let mut run: Option<u32> = None;
            for (x, s) in row.chunks_exact(4).enumerate() {
                bgra.extend_from_slice(&[s[2], s[1], s[0], s[3]]);
                match (s[3] > 0, run) {
                    (true, None) => run = Some(x as u32),
                    (false, Some(start)) => {
                        rects.push(rect(start, y as u32, x as u32 - start));
                        run = None;
                    }
                    _ => {}
                }
            }
            if let Some(start) = run {
                rects.push(rect(start, y as u32, w - start));
            }
        }
        // Send in row strips to stay under the max request size without BIG-REQUESTS.
        let rows_per_strip = (200_000 / (w * 4)).max(1);
        for (i, strip) in bgra.chunks((rows_per_strip * w * 4) as usize).enumerate() {
            let strip_h = strip.len() as u32 / (w * 4);
            self.conn
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    w as u16,
                    strip_h as u16,
                    0,
                    (i as u32 * rows_per_strip) as i16,
                    0,
                    32,
                    strip,
                )
                .unwrap();
        }
        let key: Vec<_> = rects.iter().map(|r| (r.x, r.y, r.width)).collect();
        if key != self.last_mask {
            self.conn
                .shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::YX_BANDED, self.win, 0, 0, &rects)
                .unwrap();
            self.last_mask = key;
        }
        self.conn.flush().unwrap();
    }
}

fn rect(x: u32, y: u32, width: u32) -> Rectangle {
    Rectangle { x: x as i16, y: y as i16, width: width as u16, height: 1 }
}
