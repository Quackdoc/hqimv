//! The output: the window and its surface, and when the next frame is owed.

use std::time::Instant;

use winit::window::Window;

use crate::present::Presenter;

use crate::app::wayland::WlState;

pub struct Screen {
    /// The window and its shm pool, once the event loop has resumed.
    pub wl: Option<Wl>,
    /// The window has been shown; `--hidden` holds it back until a key press.
    pub shown: bool,
    /// A repaint is owed.
    pub dirty: bool,
    /// When the animated overlays were last repainted, so they tick, not spin.
    pub last_anim: Instant,
    /// A frame the compositor refused; retried at this time.
    pub retry_at: Option<Instant>,
}

impl Screen {
    pub fn new(hidden: bool) -> Self {
        Self {
            wl: None,
            shown: !hidden,
            dirty: true,
            last_anim: Instant::now(),
            retry_at: None,
        }
    }
}

/// The window, its presentation state, and the resize it owes.
///
/// Declaration order is drop order. `presentation` must be dropped before
/// `window`: dropping the window releases winit's Wayland connection and
/// disconnects the shared `wl_display`, which invalidates the presentation's
/// proxies.
pub struct Wl {
    pub presentation: Presentation,
    pub window: Window,
    /// Surface size the compositor asked for, applied at the next draw.
    pub pending_resize: Option<(u32, u32)>,
}

/// winit only flushes after its own requests, so anything sent outside them
/// has to be flushed here or it stalls. False if the flush failed.
pub fn flush(conn: &wayland_client::Connection) -> bool {
    let result = conn.flush();
    if let Err(err) = &result {
        tracing::warn!("cannot flush the Wayland connection: {err}");
    }
    result.is_ok()
}

/// The surface, shm pool, and the queue that owns the buffers.
pub struct Presentation {
    pub conn: wayland_client::Connection,
    pub state: WlState,
    pub surface: wayland_client::protocol::wl_surface::WlSurface,
    pub presenter: Presenter,
    // Buffers belong to this queue: it must stay alive and be dispatched every
    // frame, or releases never arrive and drawing stalls.
    pub queue: wayland_client::EventQueue<WlState>,
}
