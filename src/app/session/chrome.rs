//! The window title and the transient on-screen notice.

use std::time::Instant;

use winit::window::Window;

use crate::app::host::flush;

#[derive(Default)]
pub struct Chrome {
    pub title: String,
    /// Set when the title may have changed; cleared once it is delivered.
    pub pending: bool,
    pub notice: Option<(String, Instant)>,
}

impl Chrome {
    /// Hands `text` to the compositor; false if the flush failed, so the
    /// caller knows to retry.
    pub fn commit(
        &mut self,
        text: String,
        window: &Window,
        conn: &wayland_client::Connection,
    ) -> bool {
        window.set_title(&text);
        if !flush(conn) {
            return false;
        }
        self.title = text;
        true
    }
}
