/* Custom Slint platform: software rendering into a winit window via softbuffer.
 *
 * `MinimalSoftwareWindow` (Slint's public software-rendering window adapter)
 * renders the chrome into a CPU buffer; we blit that buffer to the winit
 * window with `softbuffer`. This keeps the chrome free of GL dependencies and
 * makes the whole UI stack runnable headless (the UI smoke test uses the same
 * surface with no winit window attached).
 */

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{PhysicalSize as SlintPhysicalSize, WindowSize};

/// Pending window adapters for `BrowPlatform::create_window_adapter`.
/// Thread-local by design: window adapters are created and consumed on the
/// UI thread (`MinimalSoftwareWindow` is not `Send`).
thread_local! {
    static PENDING: RefCell<Vec<Rc<MinimalSoftwareWindow>>> = RefCell::new(Vec::new());
}

/// Queue an adapter for the next Slint component creation (UI thread).
pub fn queue_global(window: Rc<MinimalSoftwareWindow>) {
    PENDING.with_borrow_mut(|q| q.push(window));
}

/// 32-bit XRGB pixel for softbuffer presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Xrgb8888(pub u32);

impl TargetPixel for Xrgb8888 {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let a = u16::from(color.alpha);
        if a == 0 {
            return;
        }
        if a == u16::from(u8::MAX) {
            *self = Self::from_rgb(color.red, color.green, color.blue);
            return;
        }
        let inv = 255 - a;
        let blend = |dst: u8, src: u8| -> u8 {
            let v = u16::from(src) + (u16::from(dst) * inv + 127) / 255;
            v.min(255) as u8
        };
        let (dr, dg, db) = channels(self.0);
        self.0 = 0xFF00_0000
            | (u32::from(blend(dr, color.red)) << 16)
            | (u32::from(blend(dg, color.green)) << 8)
            | u32::from(blend(db, color.blue));
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self(0xFF00_0000 | (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue))
    }
}

fn channels(px: u32) -> (u8, u8, u8) {
    ((px >> 16) as u8, (px >> 8) as u8, px as u8)
}

/// The brow Slint platform.
pub struct BrowPlatform;

impl Platform for BrowPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        PENDING
            .with_borrow_mut(|q| q.pop())
            .map(|w| w as Rc<dyn WindowAdapter>)
            .ok_or_else(|| {
                PlatformError::Other(
                    "no queued brow window adapter: create the winit window before the Slint \
                     component"
                        .into(),
                )
            })
    }
}

/// Softbuffer state attached to a real winit window. The `Window` is moved
/// into the `Surface`; sizes flow in via `resize` from winit events.
struct Attached {
    context: softbuffer::Context<raw_window_handle::DisplayHandle<'static>>,
    surface: softbuffer::Surface<raw_window_handle::DisplayHandle<'static>, Window>,
}

use winit::window::Window;

/// A chrome surface: Slint window + CPU framebuffer, optionally blitted to a
/// winit window through softbuffer.
pub struct ChromeSurface {
    pub slint_window: Rc<MinimalSoftwareWindow>,
    pub last_cursor: Cell<(f64, f64)>,
    softbuffer: RefCell<Option<Attached>>,
    buffer: RefCell<Vec<Xrgb8888>>,
    width: Cell<u32>,
    height: Cell<u32>,
}

impl ChromeSurface {
    /// Surface attached to a real winit window (frames are presented). The
    /// window is consumed by the softbuffer surface; the display handle comes
    /// from the winit event loop.
    pub fn new_attached(
        window: Window,
        display_handle: raw_window_handle::DisplayHandle<'static>,
    ) -> Self {
        let slint_window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        let size = window.inner_size();
        let context = softbuffer::Context::new(display_handle).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window).expect("softbuffer surface");
        let out = Self {
            slint_window,
            last_cursor: Cell::new((0.0, 0.0)),
            softbuffer: RefCell::new(Some(Attached { context, surface })),
            buffer: RefCell::new(Vec::new()),
            width: Cell::new(0),
            height: Cell::new(0),
        };
        out.resize(size.width.max(1), size.height.max(1));
        out
    }

    /// Headless surface for tests (frames render into the CPU buffer only).
    pub fn new_headless() -> Self {
        let surface = Self {
            slint_window: MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer),
            last_cursor: Cell::new((0.0, 0.0)),
            softbuffer: RefCell::new(None),
            buffer: RefCell::new(Vec::new()),
            width: Cell::new(0),
            height: Cell::new(0),
        };
        surface.resize(1240, 88);
        surface
    }

    /// Resize the framebuffer (physical pixels) and tell Slint.
    pub fn resize(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width.set(width);
        self.height.set(height);
        self.buffer
            .borrow_mut()
            .resize((width * height) as usize, Xrgb8888::from_rgb(0x14, 0x16, 0x1c));
        self.slint_window
            .window()
            .set_size(WindowSize::Physical(SlintPhysicalSize::new(width, height)));
    }

    /// Read-only snapshot of the framebuffer (for tests / screenshots).
    pub fn buffer_snapshot(&self) -> Vec<Xrgb8888> {
        self.buffer.borrow().clone()
    }

    /// Render a frame if Slint needs one, then present to the window (when
    /// attached). Returns true when a new frame was produced.
    pub fn draw_if_needed(&self) -> bool {
        let (w, h) = (self.width.get(), self.height.get());
        if w == 0 || h == 0 {
            return false;
        }
        let mut buffer = self.buffer.borrow_mut();
        let drew = self.slint_window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer[..], w as usize);
        });
        drop(buffer);
        if drew {
            self.present();
        }
        drew
    }

    fn present(&self) {
        let mut softbuffer = self.softbuffer.borrow_mut();
        let Some(attached) = softbuffer.as_mut() else {
            return;
        };
        let (w, h) = (self.width.get(), self.height.get());
        let Ok(()) = attached.surface.resize(
            std::num::NonZeroU32::new(w).unwrap(),
            std::num::NonZeroU32::new(h).unwrap(),
        ) else {
            return;
        };
        let Ok(mut target) = attached.surface.buffer_mut() else {
            return;
        };
        let buffer = self.buffer.borrow();
        for (i, px) in buffer.iter().enumerate() {
            if i < target.len() {
                target[i] = px.0 & 0x00FF_FFFF;
            }
        }
        let _ = target.present();
    }
}

/// Slint must be pumped for timers/animations while we own the event loop.
pub fn pump_slint_timers() {
    slint::platform::update_timers_and_animations();
}

pub fn duration_until_next_slint_update() -> Option<std::time::Duration> {
    slint::platform::duration_until_next_timer_update()
}
