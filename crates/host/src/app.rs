//! winit 0.30 `ApplicationHandler` plumbing and pure event mapping.
//!
//! The host API is poll-style: winit's own loop is pumped from the
//! outside (`pump_app_events`), and every winit event of interest is
//! translated into a [`HostEvent`] queued for the owner. `resumed`
//! creates the window plus the softbuffer context and surface;
//! `present` copies the shared `x8r8g8b8` buffer into the softbuffer
//! buffer behind `window.pre_present_notify()`.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::{HostError, HostEvent};

/// plan9 button masks (devdraw `m.buttons`): 1 left, 2 middle, 4 right;
/// 8/16 are the plan9port extension for extra buttons and the wheel
/// (acme scrolls on 8/16). Unknown buttons are dropped.
fn map_button(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0x01),
        MouseButton::Middle => Some(0x02),
        MouseButton::Right => Some(0x04),
        MouseButton::Back => Some(0x08),
        MouseButton::Forward => Some(0x10),
        MouseButton::Other(_) => None,
    }
}

/// Vertical scroll -> plan9port wheel buttons: away from the user
/// (`y > 0`) is 8, towards (`y < 0`) is 16, no vertical motion -> None.
fn map_scroll(delta: MouseScrollDelta) -> Option<u8> {
    let dy = match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(p) => p.y,
    };
    if dy > 0.0 {
        Some(0x08)
    } else if dy < 0.0 {
        Some(0x10)
    } else {
        None
    }
}

/// Window-local physical cursor coordinates: round and clamp into the
/// screen rect `0..w-1 x 0..h-1` (plan9 rects: max is exclusive).
/// Non-finite input clamps to the origin.
fn map_cursor(x: f64, y: f64, w: u32, h: u32) -> (i32, i32) {
    let max_x = (w.max(1) as i64 - 1) as f64;
    let max_y = (h.max(1) as i64 - 1) as f64;
    let rx = if x.is_finite() { x.round() } else { 0.0 };
    let ry = if y.is_finite() { y.round() } else { 0.0 };
    (rx.clamp(0.0, max_x) as i32, ry.clamp(0.0, max_y) as i32)
}

/// Byte length of an `x8r8g8b8` frame: `w * h * 4` (saturating).
fn frame_len(w: u32, h: u32) -> usize {
    (w as usize).saturating_mul(h as usize).saturating_mul(4)
}

/// Softbuffer state; `size` tracks the last surface `resize`.
struct SoftGpu {
    _context: Context<Arc<Window>>,
    surface: Surface<Arc<Window>, Arc<Window>>,
    size: (u32, u32),
}

impl SoftGpu {
    fn new(window: Arc<Window>) -> Result<SoftGpu, HostError> {
        let context: Context<Arc<Window>> =
            Context::new(window.clone()).map_err(|e| HostError::WindowInit(e.to_string()))?;
        let surface: Surface<Arc<Window>, Arc<Window>> =
            Surface::new(&context, window).map_err(|e| HostError::WindowInit(e.to_string()))?;
        Ok(SoftGpu {
            _context: context,
            surface,
            size: (0, 0),
        })
    }
}

/// winit application state; driven only through crate-visible methods by
/// [`crate::ScreenHost`].
pub(crate) struct HostApp {
    title: String,
    width: u32,
    height: u32,
    buttons: u8,
    cursor: Option<(i32, i32)>,
    window: Option<Arc<Window>>,
    gpu: Option<SoftGpu>,
    buf: Vec<u8>,
    queue: VecDeque<HostEvent>,
    error: Option<HostError>,
    closed: bool,
}

impl HostApp {
    pub(crate) fn new(title: String, width: u32, height: u32) -> HostApp {
        HostApp {
            title,
            width,
            height,
            buttons: 0,
            cursor: None,
            window: None,
            gpu: None,
            buf: Vec::new(),
            queue: VecDeque::new(),
            error: None,
            closed: false,
        }
    }

    /// `resumed` ran and both the window and softbuffer came up.
    pub(crate) fn window_created(&self) -> bool {
        self.window.is_some() && self.error.is_none()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    pub(crate) fn take_error(&mut self) -> Option<HostError> {
        self.error.take()
    }

    /// The shared client-drawable buffer, sized exactly `w * h * 4` for
    /// the current window size (grown zero-filled, shrunk as needed).
    pub(crate) fn surface_slice(&mut self) -> &mut [u8] {
        let want = frame_len(self.width, self.height);
        if self.buf.len() != want {
            self.buf.resize(want, 0);
        }
        &mut self.buf
    }

    /// Copy the shared buffer into the softbuffer buffer and publish it.
    /// No-op before the window exists, after close, or for an empty
    /// frame; a stale-size frame is zero-padded to the window size so a
    /// resize can never present out-of-bounds bytes.
    pub(crate) fn present(&mut self) {
        if self.closed || self.buf.is_empty() {
            return;
        }
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let (w, h) = (self.width.max(1), self.height.max(1));
        let want = frame_len(w, h);
        if self.buf.len() != want {
            self.buf.resize(want, 0);
        }
        if gpu.size != (w, h) {
            let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
                return;
            };
            if let Err(e) = gpu.surface.resize(nw, nh) {
                self.error = Some(HostError::Present(e.to_string()));
                return;
            }
            gpu.size = (w, h);
        }
        window.pre_present_notify();
        match gpu.surface.buffer_mut() {
            Ok(mut buffer) => {
                // SAFETY: softbuffer's buffer is LE u32 (0x00RRGGBB); its
                // little-endian byte view is exactly the documented surface
                // layout [B, G, R, X] per pixel (x86_64 target).
                let px32: &mut [u32] = &mut *buffer;
                let px: &mut [u8] = unsafe {
                    std::slice::from_raw_parts_mut(px32.as_mut_ptr().cast::<u8>(), px32.len() * 4)
                };
                let len = px.len().min(self.buf.len());
                px[..len].copy_from_slice(&self.buf[..len]);
                if let Err(e) = buffer.present() {
                    self.error = Some(HostError::Present(e.to_string()));
                }
            }
            Err(e) => self.error = Some(HostError::Present(e.to_string())),
        }
    }

    pub(crate) fn drain_events(&mut self) -> Vec<HostEvent> {
        self.queue.drain(..).collect()
    }
}

impl ApplicationHandler for HostApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // re-resume after suspend: keep the existing window
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(self.width, self.height));
        match event_loop.create_window(attrs) {
            Ok(window) => {
                // Released winit 0.30.x returns the `Window` struct; wrap it
                // in `Arc<Window>` (softbuffer needs an owned handle).
                let window: Arc<Window> = window.into();
                match SoftGpu::new(window.clone()) {
                    Ok(gpu) => {
                        self.window = Some(window);
                        self.gpu = Some(gpu);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Err(e) => self.error = Some(HostError::WindowInit(e.to_string())),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let (w, h) = (size.width, size.height);
                // Wayland reports 0x0 while minimized; keep the last size.
                if w > 0 && h > 0 && (w, h) != (self.width, self.height) {
                    self.width = w;
                    self.height = h;
                    self.queue.push_back(HostEvent::Resize { w, h });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = map_cursor(position.x, position.y, self.width, self.height);
                self.cursor = Some((x, y));
                self.queue.push_back(HostEvent::Mouse {
                    x,
                    y,
                    buttons: self.buttons,
                });
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(mask) = map_button(button) {
                    match state {
                        ElementState::Pressed => self.buttons |= mask,
                        ElementState::Released => self.buttons &= !mask,
                    }
                    if let Some((x, y)) = self.cursor {
                        self.queue.push_back(HostEvent::Mouse {
                            x,
                            y,
                            buttons: self.buttons,
                        });
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(mask) = map_scroll(delta) {
                    // plan9port delivers a wheel tick as a press/release
                    // pair of the wheel button (8 or 16).
                    if let Some((x, y)) = self.cursor {
                        self.queue.push_back(HostEvent::Mouse {
                            x,
                            y,
                            buttons: mask,
                        });
                        self.queue.push_back(HostEvent::Mouse {
                            x,
                            y,
                            buttons: self.buttons,
                        });
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // Synthetic releases (X11 Alt+NumLock) carry no text and
                // must not reach the client as keystrokes.
                if is_synthetic || event.state != ElementState::Pressed {
                    return;
                }
                if let Some(c) = event.text.as_ref().and_then(|text| text.chars().next()) {
                    self.queue.push_back(HostEvent::Key(c));
                }
            }
            WindowEvent::CloseRequested => {
                self.closed = true;
                self.queue.push_back(HostEvent::Close);
                event_loop.exit();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{frame_len, map_button, map_cursor, map_scroll};
    use winit::event::{MouseButton, MouseScrollDelta};

    #[test]
    fn button_masks_match_plan9() {
        assert_eq!(map_button(MouseButton::Left), Some(0x01));
        assert_eq!(map_button(MouseButton::Middle), Some(0x02));
        assert_eq!(map_button(MouseButton::Right), Some(0x04));
        assert_eq!(map_button(MouseButton::Back), Some(0x08));
        assert_eq!(map_button(MouseButton::Forward), Some(0x10));
        assert_eq!(map_button(MouseButton::Other(9)), None);
    }

    #[test]
    fn scroll_maps_to_wheel_buttons() {
        assert_eq!(map_scroll(MouseScrollDelta::LineDelta(0.0, 1.0)), Some(0x08));
        assert_eq!(map_scroll(MouseScrollDelta::LineDelta(0.0, -1.0)), Some(0x10));
        assert_eq!(map_scroll(MouseScrollDelta::LineDelta(0.0, 0.0)), None);
        assert_eq!(
            map_scroll(MouseScrollDelta::PixelDelta((0.0, -42.0).into())),
            Some(0x10)
        );
        assert_eq!(
            map_scroll(MouseScrollDelta::PixelDelta((0.0, 3.5).into())),
            Some(0x08)
        );
    }

    #[test]
    fn cursor_rounds_and_clamps_to_rect() {
        assert_eq!(map_cursor(10.4, 5.6, 100, 50), (10, 6));
        assert_eq!(map_cursor(-3.0, -0.2, 100, 50), (0, 0));
        assert_eq!(map_cursor(1000.0, 1000.0, 100, 50), (99, 49));
        assert_eq!(map_cursor(f64::NAN, f64::INFINITY, 100, 50), (0, 0));
    }

    #[test]
    fn frame_math_and_zero_size() {
        assert_eq!(map_cursor(0.0, 0.0, 0, 0), (0, 0));
        assert_eq!(frame_len(0, 0), 0);
        assert_eq!(frame_len(4, 3), 48);
        assert_eq!(frame_len(u32::MAX, u32::MAX), usize::MAX);
    }
}
