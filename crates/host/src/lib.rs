//! p9draw-host — local window host for p9draw (brick 1: paint).
//!
//! A small synchronous API over winit 0.30 (windowing) and softbuffer 0.4
//! (CPU raster presentation), shaped so the future p9draw-server can drive
//! a real screen from its dispatch loop:
//!
//! - [`ScreenHost::open`] creates the window; the first non-blocking pump
//!   of the internal winit loop has to come back with a window, otherwise
//!   this fails (headless hosts have no display at all);
//! - [`ScreenHost::surface`] hands out the client-drawable pixel buffer in
//!   `x8r8g8b8` — `w * h * 4` bytes, byte order B, G, R, X (little-endian
//!   `0x00RRGGBB`), the layout X11 ZPixmap and softbuffer both expect
//!   (SPEC.md §7);
//! - [`ScreenHost::present`] publishes the buffer through softbuffer,
//!   preceded by `window.pre_present_notify()`;
//! - [`ScreenHost::poll_events`] pumps the winit loop once, non-blocking,
//!   and drains the queued [`HostEvent`]s.
//!
//! Winit cannot run on a headless host, so unit tests cover only the pure
//! event-mapping helpers (button masks, wheel, cursor rounding/clamping);
//! everything else is compile-checked.

mod app;

use std::fmt;
use std::time::Duration;

use app::HostApp;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;

/// Host-side event, translated from winit.
///
/// Mouse coordinates are window-local physical pixels, rounded and
/// clamped into `0..w-1 × 0..h-1` (plan9 rects: max is exclusive).
/// `buttons` is the plan9 devdraw mask: 1 = left, 2 = middle, 4 = right;
/// 8 and 16 are the plan9port extension used for wheel/extra buttons
/// (acme scrolls on 8/16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostEvent {
    /// Pointer move or button change at (`x`, `y`).
    Mouse {
        x: i32,
        y: i32,
        buttons: u8,
    },
    /// Text character from a key press (winit `KeyEvent::text`).
    Key(char),
    /// Window resized to `w` × `h` physical pixels.
    Resize {
        w: u32,
        h: u32,
    },
    /// Close requested (window X button / WM_DELETE_WINDOW).
    Close,
}

/// Window-host construction/runtime errors.
///
/// Foreign (winit/softbuffer) errors are stringified: their exact types
/// drift between minor releases and no caller can match on them anyway.
#[derive(Debug)]
pub enum HostError {
    /// The winit event loop could not be created.
    EventLoop(String),
    /// The window or the softbuffer context/surface failed to initialize.
    WindowInit(String),
    /// Presenting a frame failed.
    Present(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::EventLoop(e) => write!(f, "event loop: {e}"),
            HostError::WindowInit(e) => write!(f, "window init: {e}"),
            HostError::Present(e) => write!(f, "present: {e}"),
        }
    }
}

impl std::error::Error for HostError {}

/// A local screen window with a client-drawable `x8r8g8b8` pixel buffer.
///
/// Winit is driven through its poll-style extension
/// (`winit::platform::pump_events`): every [`ScreenHost::poll_events`]
/// call pumps the internal event loop without blocking, so the owner
/// keeps both the thread and the frame cadence.
pub struct ScreenHost {
    event_loop: EventLoop<()>,
    app: HostApp,
}

impl ScreenHost {
    /// Open a window titled `title`, `w` × `h` logical pixels.
    ///
    /// Pumps the loop once so winit's `resumed` callback creates the
    /// window (and the softbuffer surface) synchronously; fails when
    /// there is no display or initialization errors out.
    pub fn open(title: &str, w: u32, h: u32) -> Result<ScreenHost, HostError> {
        let mut event_loop = EventLoop::new().map_err(|e| HostError::EventLoop(e.to_string()))?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut app = HostApp::new(title.to_owned(), w.max(1), h.max(1));
        let _ = event_loop.pump_app_events(Some(Duration::ZERO), &mut app);
        if let Some(err) = app.take_error() {
            return Err(err);
        }
        if app.window_created() {
            Ok(ScreenHost { event_loop, app })
        } else {
            Err(HostError::WindowInit(
                "window was not created during startup (no display?)".to_owned(),
            ))
        }
    }

    /// The client-drawable frame buffer, exactly `w * h * 4` bytes of
    /// `x8r8g8b8` (B, G, R, X per pixel) for the current window size.
    /// Grown (zero-filled) or shrunk to match; draw a fresh frame after
    /// a [`HostEvent::Resize`].
    pub fn surface(&mut self) -> &mut [u8] {
        self.app.surface_slice()
    }

    /// Publish the buffer: resize the softbuffer surface on demand, copy
    /// the frame in and present it (behind `pre_present_notify`).
    /// No-op before the window exists or after close.
    pub fn present(&mut self) {
        self.app.present();
    }

    /// Pump the internal winit loop once (non-blocking) and return the
    /// queued events in arrival order. After [`HostEvent::Close`] the
    /// loop is shut down and later calls only drain leftovers.
    pub fn poll_events(&mut self) -> Vec<HostEvent> {
        if !self.app.is_closed() {
            let _ = self.event_loop.pump_app_events(Some(Duration::ZERO), &mut self.app);
        }
        self.app.drain_events()
    }
}
