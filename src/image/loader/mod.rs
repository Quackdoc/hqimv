//! Decoding images into linear light: one still, or every animation frame.

use std::path::Path;

use image::ImageReader;

use super::animation::Animation;
use super::buffer::WorkingImage;
use crate::playlist::is_url;

mod decode;
mod error;
mod http;
mod options;

pub use error::LoadError;
pub use options::LoadOptions;

pub enum Decoded {
    Still(WorkingImage),
    Animated(Animation),
}

/// Decodes `path` to linear light, applying EXIF orientation.
///
/// An `http://` or `https://` path is fetched; anything else is opened as a file.
pub fn load(path: &Path, opts: &LoadOptions) -> Result<Decoded, LoadError> {
    if is_url(path) {
        http::load(path, opts)
    } else {
        let reader = ImageReader::open(path).map_err(|source| LoadError::Io {
            path: path.display().to_string(),
            source,
        })?;
        decode::decode(reader, path, opts)
    }
}
