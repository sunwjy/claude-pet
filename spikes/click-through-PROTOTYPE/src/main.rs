//! PROTOTYPE — throwaway spike for issue #10 (click-through 투명 창 spike).
//! Answers: does a transparent always-on-top pet window with pixel click-through
//! and drag work on the recommended stack? Not production code; delete freely.
//!
//! Run: `cargo run --release`. Everything interesting is printed to stdout.
//! Switch modes / pause animation / quit from the tray icon menu.

use std::time::{Duration, Instant};

use resvg::tiny_skia::{Pixmap, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

#[cfg(target_os = "linux")]
#[path = "x11.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "mac.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "win.rs"]
mod platform;

const WIN_LOGICAL: f64 = 200.0;
const FPS: u64 = 30;
const DRAG_THRESHOLD_PX: f64 = 4.0;
/// Hitbox of the body in logical coords (x, y, w, h) — used by TwoWindow mode.
const HITBOX: (f64, f64, f64, f64) = (40.0, 70.0, 120.0, 110.0);

/// Original placeholder character: a round body plus a *separate* floating
/// orb. The gap between them is the pixel-accuracy test: in pixel modes it
/// must click through, in hitbox mode the orb is not clickable at all.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <circle cx="150" cy="35" r="16" fill="#ffb347"/>
  <ellipse cx="100" cy="125" rx="58" ry="52" fill="#6ec6ff"/>
  <ellipse cx="100" cy="125" rx="58" ry="52" fill="none" stroke="#2a6f97" stroke-width="4"/>
  <circle cx="80" cy="115" r="8" fill="#123"/>
  <circle cx="120" cy="115" r="8" fill="#123"/>
  <path d="M85 145 Q100 158 115 145" stroke="#123" stroke-width="4" fill="none" stroke-linecap="round"/>
  <rect x="60" y="100" width="80" height="6" fill="none"/>
</svg>"##;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Hit-test always on; rely on the OS to pass alpha-0 pixels through
    /// (Windows layered windows, X11 input shape, macOS window server?).
    Native,
    /// macOS: poll the global cursor and toggle `set_cursor_hittest` by alpha.
    Poll,
    /// clawd-on-desk style: render window always click-through, a separate
    /// hitbox-sized "hit window" follows it and takes the input.
    TwoWindow,
}

struct Press {
    at: PhysicalPosition<f64>,
    dragging: bool,
    window: WindowId,
}

struct App {
    t0: Instant,
    tree: resvg::usvg::Tree,
    window: Option<Window>,
    presenter: Option<platform::Presenter>,
    hit_window: Option<Window>,
    hit_presenter: Option<platform::Presenter>,
    pixmap: Option<Pixmap>,
    mode: Mode,
    paused: bool,
    hittest_on: bool,
    hittest_on_since: Option<Instant>,
    toggles: u32,
    cursor: PhysicalPosition<f64>,
    press: Option<Press>,
    presses: u32,
    clicks: u32,
    reaction_until: Option<Instant>,
    next_frame: Instant,
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    tray: Option<Tray>,
}

macro_rules! log {
    ($app:expr, $($arg:tt)*) => {
        println!("[{:>8.3}s] {}", $app.t0.elapsed().as_secs_f64(), format!($($arg)*))
    };
}

impl App {
    fn new() -> Self {
        let tree = resvg::usvg::Tree::from_str(SVG, &resvg::usvg::Options::default()).unwrap();
        Self {
            t0: Instant::now(),
            tree,
            window: None,
            presenter: None,
            hit_window: None,
            hit_presenter: None,
            pixmap: None,
            mode: if cfg!(target_os = "macos") { Mode::Poll } else { Mode::Native },
            paused: false,
            hittest_on: true,
            hittest_on_since: None,
            toggles: 0,
            cursor: PhysicalPosition::new(0.0, 0.0),
            press: None,
            presses: 0,
            clicks: 0,
            reaction_until: None,
            next_frame: Instant::now(),
            tray: None,
        }
    }

    fn render(&mut self) {
        let window = self.window.as_ref().unwrap();
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let pixmap = self
            .pixmap
            .get_or_insert_with(|| Pixmap::new(size.width, size.height).unwrap());
        if pixmap.width() != size.width || pixmap.height() != size.height {
            *pixmap = Pixmap::new(size.width, size.height).unwrap();
        }
        pixmap.fill(resvg::tiny_skia::Color::TRANSPARENT);

        let t = self.t0.elapsed().as_secs_f32();
        let bob = if self.paused { 0.0 } else { (t * 3.0).sin() * 6.0 };
        let squash = match self.reaction_until {
            Some(until) if Instant::now() < until => 0.85f32,
            _ => 1.0,
        };
        // Squash around the feet (100, 177) in logical coords.
        let tf = Transform::from_scale(scale, scale)
            .pre_translate(0.0, bob)
            .pre_translate(100.0, 177.0)
            .pre_scale(1.0 / squash.max(0.01).sqrt(), squash)
            .pre_translate(-100.0, -177.0);
        resvg::render(&self.tree, tf, &mut pixmap.as_mut());

        self.presenter.as_mut().unwrap().present(window, pixmap);
    }

    fn alpha_at(&self, x: f64, y: f64) -> u8 {
        let Some(p) = &self.pixmap else { return 0 };
        if x < 0.0 || y < 0.0 || x >= p.width() as f64 || y >= p.height() as f64 {
            return 0;
        }
        p.pixel(x as u32, y as u32).map(|c| c.alpha()).unwrap_or(0)
    }

    fn set_hittest(&mut self, on: bool) {
        if self.hittest_on == on {
            return;
        }
        self.hittest_on = on;
        self.toggles += 1;
        self.hittest_on_since = on.then(Instant::now);
        let _ = self.window.as_ref().unwrap().set_cursor_hittest(on);
        log!(self, "hittest {} (toggle #{})", if on { "ON " } else { "off" }, self.toggles);
    }

    fn apply_mode(&mut self, event_loop: &ActiveEventLoop) {
        log!(self, "=== mode: {:?} ===", self.mode);
        self.hit_window = None;
        self.hit_presenter = None;
        match self.mode {
            Mode::Native => {
                self.hittest_on = false;
                self.set_hittest(true);
            }
            Mode::Poll => {}
            Mode::TwoWindow => {
                self.hittest_on = true;
                self.set_hittest(false);
                let w = event_loop
                    .create_window(base_attrs("hit").with_inner_size(LogicalSize::new(HITBOX.2, HITBOX.3)))
                    .unwrap();
                let mut presenter = platform::Presenter::new(&w);
                let size = w.inner_size();
                let mut p = Pixmap::new(size.width, size.height).unwrap();
                // alpha 1/255: invisible but still hit-testable. Set DEBUG_HITBOX=1 to see it.
                let a = if std::env::var_os("DEBUG_HITBOX").is_some() { 60 } else { 1 };
                p.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, a));
                presenter.present(&w, &p);
                self.hit_window = Some(w);
                self.hit_presenter = Some(presenter);
                self.follow_hit_window_to_render();
            }
        }
    }

    fn follow_hit_window_to_render(&self) {
        let (Some(w), Some(hw)) = (&self.window, &self.hit_window) else { return };
        let Ok(pos) = w.outer_position() else { return };
        let s = w.scale_factor();
        hw.set_outer_position(PhysicalPosition::new(
            pos.x + (HITBOX.0 * s) as i32,
            pos.y + (HITBOX.1 * s) as i32,
        ));
    }

    fn follow_render_to_hit_window(&self) {
        let (Some(w), Some(hw)) = (&self.window, &self.hit_window) else { return };
        let Ok(pos) = hw.outer_position() else { return };
        let s = w.scale_factor();
        w.set_outer_position(PhysicalPosition::new(
            pos.x - (HITBOX.0 * s) as i32,
            pos.y - (HITBOX.1 * s) as i32,
        ));
    }

    #[cfg(target_os = "macos")]
    fn poll_cursor(&mut self) {
        if self.mode != Mode::Poll {
            return;
        }
        let w = self.window.as_ref().unwrap();
        let (x, y) = platform::cursor_in_window_px(w);
        let hit = self.alpha_at(x, y) > 0;
        self.set_hittest(hit);
    }

    #[cfg(not(target_os = "macos"))]
    fn poll_cursor(&mut self) {}

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop) {
        let Some(tray) = &self.tray else { return };
        while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            let id = ev.id;
            if id == tray.quit {
                self.report();
                event_loop.exit();
            } else if id == tray.pause {
                self.paused = !self.paused;
                log!(self, "animation {}", if self.paused { "PAUSED (no redraws)" } else { "running" });
                self.render();
            } else if id == tray.native {
                self.mode = Mode::Native;
                self.apply_mode(event_loop);
            } else if id == tray.poll {
                self.mode = Mode::Poll;
                self.apply_mode(event_loop);
            } else if id == tray.two {
                self.mode = Mode::TwoWindow;
                self.apply_mode(event_loop);
            } else if id == tray.report {
                self.report();
            }
            return;
        }
    }

    fn report(&self) {
        log!(
            self,
            "REPORT mode={:?} presses={} clicks={} hittest_toggles={}",
            self.mode, self.presses, self.clicks, self.toggles
        );
    }
}

fn base_attrs(title: &str) -> winit::window::WindowAttributes {
    let attrs = Window::default_attributes()
        .with_title(title)
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(WindowLevel::AlwaysOnTop);
    #[cfg(target_os = "macos")]
    let attrs = {
        use winit::platform::macos::WindowAttributesExtMacOS;
        attrs.with_has_shadow(false)
    };
    #[cfg(target_os = "windows")]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_skip_taskbar(true)
    };
    attrs
}

struct Tray {
    _icon: tray_icon::TrayIcon,
    native: tray_icon::menu::MenuId,
    poll: tray_icon::menu::MenuId,
    two: tray_icon::menu::MenuId,
    pause: tray_icon::menu::MenuId,
    report: tray_icon::menu::MenuId,
    quit: tray_icon::menu::MenuId,
}

fn make_tray() -> Tray {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    let native = MenuItem::new("Mode: Native (OS alpha hit-test)", true, None);
    let poll = MenuItem::new("Mode: Poll (macOS cursor polling)", cfg!(target_os = "macos"), None);
    let two = MenuItem::new("Mode: TwoWindow (hitbox window)", true, None);
    let pause = MenuItem::new("Pause / resume animation", true, None);
    let report = MenuItem::new("Print report", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[
        &native,
        &poll,
        &two,
        &PredefinedMenuItem::separator(),
        &pause,
        &report,
        &quit,
    ])
    .unwrap();
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            if dx * dx + dy * dy < 14.0 * 14.0 {
                rgba[(y * 32 + x) * 4..][..4].copy_from_slice(&[0x6e, 0xc6, 0xff, 0xff]);
            }
        }
    }
    let icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("click-through spike (PROTOTYPE)")
        .with_icon(tray_icon::Icon::from_rgba(rgba, 32, 32).unwrap())
        .build()
        .unwrap();
    Tray {
        _icon: icon,
        native: native.id().clone(),
        poll: poll.id().clone(),
        two: two.id().clone(),
        pause: pause.id().clone(),
        report: report.id().clone(),
        quit: quit.id().clone(),
    }
}

impl ApplicationHandler for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // tray-icon on macOS must be created after the event loop has started.
        if cause == StartCause::Init {
            self.tray = Some(make_tray());
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let w = event_loop
            .create_window(
                base_attrs("click-through spike (PROTOTYPE)")
                    .with_inner_size(LogicalSize::new(WIN_LOGICAL, WIN_LOGICAL)),
            )
            .unwrap();
        log!(self, "window created, scale factor {}, size {:?}", w.scale_factor(), w.inner_size());
        self.presenter = Some(platform::Presenter::new(&w));
        self.window = Some(w);
        self.render();
        self.apply_mode(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let is_hit = self.hit_window.as_ref().map(|w| w.id()) == Some(id);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Moved(_) if is_hit => self.follow_render_to_hit_window(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if let Some(press) = &mut self.press {
                    let (dx, dy) = (position.x - press.at.x, position.y - press.at.y);
                    if !press.dragging && (dx * dx + dy * dy).sqrt() > DRAG_THRESHOLD_PX {
                        press.dragging = true;
                        let target = if press.window == id && is_hit {
                            self.hit_window.as_ref()
                        } else {
                            self.window.as_ref()
                        };
                        let r = target.unwrap().drag_window();
                        log!(self, "drag start ({:?})", r);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => {
                    self.presses += 1;
                    let since = self
                        .hittest_on_since
                        .map(|t| format!("{:.0}ms after hittest ON", t.elapsed().as_secs_f64() * 1000.0))
                        .unwrap_or_else(|| "hittest ON since start".into());
                    let a = if is_hit { 255 } else { self.alpha_at(self.cursor.x, self.cursor.y) };
                    log!(
                        self,
                        "press #{} on {} at ({:.0},{:.0}) alpha={} [{}]",
                        self.presses,
                        if is_hit { "HIT window" } else { "render window" },
                        self.cursor.x,
                        self.cursor.y,
                        a,
                        since
                    );
                    self.press = Some(Press { at: self.cursor, dragging: false, window: id });
                }
                ElementState::Released => {
                    if let Some(press) = self.press.take() {
                        if press.dragging {
                            log!(self, "drag end");
                            // winit on macOS may swallow the release after drag_window.
                        } else {
                            self.clicks += 1;
                            log!(self, "CLICK #{} -> reaction", self.clicks);
                            self.reaction_until = Some(Instant::now() + Duration::from_millis(250));
                        }
                    }
                }
            },
            WindowEvent::RedrawRequested if !is_hit => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_menu(event_loop);
        // Drag on macOS runs a modal loop and may never deliver Released.
        if self.press.as_ref().is_some_and(|p| p.dragging) {
            self.press = None;
        }
        self.poll_cursor();
        let now = Instant::now();
        let animating = !self.paused || self.reaction_until.is_some_and(|u| now < u + Duration::from_millis(50));
        if animating && now >= self.next_frame {
            self.render();
            self.next_frame = now + Duration::from_millis(1000 / FPS);
        }
        let poll_needed = self.mode == Mode::Poll && cfg!(target_os = "macos");
        let wake = if animating {
            Some(self.next_frame)
        } else if poll_needed {
            None
        } else {
            // Nothing to animate or poll: sleep until an OS event (true idle).
            event_loop.set_control_flow(ControlFlow::Wait);
            // Tray menu events don't wake winit; check them a few times a second.
            Some(now + Duration::from_millis(250))
        };
        let wake = match (wake, poll_needed) {
            (Some(w), true) => w.min(now + Duration::from_millis(16)),
            (None, true) => now + Duration::from_millis(16),
            (Some(w), false) => w,
            (None, false) => unreachable!(),
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}

fn main() {
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "linux")]
    {
        // Wayland: force XWayland (best-effort per issue #2).
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11();
    }
    let event_loop = builder.build().unwrap();
    let mut app = App::new();
    println!("PROTOTYPE click-through spike — pid {}", std::process::id());
    event_loop.run_app(&mut app).unwrap();
    app.report();
}
