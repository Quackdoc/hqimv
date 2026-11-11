//! What can go wrong loading an image.

use std::io;
use std::path::Path;

use image::ImageError;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot open {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("cannot decode {path}: {source}")]
    Decode {
        path: String,
        #[source]
        source: ImageError,
    },
    #[error("{path} exceeds the decode limits: {reason}")]
    Limits { path: String, reason: String },
    #[error("cannot fetch {path}: {message}")]
    Http { path: String, message: String },
}

impl LoadError {
    pub fn limits(path: &Path, reason: impl Into<String>) -> Self {
        Self::Limits {
            path: path.display().to_string(),
            reason: reason.into(),
        }
    }
}
