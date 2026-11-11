//! Errors that stop the viewer from starting.

use std::io;

use raw_window_handle::HandleError;
use wayland_client::DispatchError;
use wayland_client::globals::{BindError, GlobalError};

use crate::config::ConfigError;
use crate::present::PresentError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("no images to show")]
    NoImages,
    #[error("cannot read {what}: {source}")]
    ReadList {
        what: String,
        #[source]
        source: io::Error,
    },
    #[error("cannot create a window: {0}")]
    Window(#[from] winit::error::OsError),
    #[error("the window has no native handle: {0}")]
    Handle(#[from] HandleError),
    #[error("hqimv presents through wl_shm and needs a Wayland session")]
    NotWayland,
    #[error("the window is not a live wl_surface")]
    NoSurface,
    #[error("cannot list the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("the compositor has no usable wl_shm: {0}")]
    Shm(#[from] BindError),
    #[error("the compositor round trip failed: {0}")]
    Roundtrip(#[from] DispatchError),
    #[error("unknown format `{0}`")]
    UnknownFormat(String),
    #[error("the compositor advertised no wl_shm format we can encode")]
    NoFormat,
    #[error(transparent)]
    Present(#[from] PresentError),
}
