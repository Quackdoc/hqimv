//! One file in the playlist.

use std::path::PathBuf;
use std::time::SystemTime;

use super::paths::{absolutize, display_name, is_url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub path: PathBuf,
    /// File size in bytes, 0 if stat failed.
    pub size: u64,
    /// Modification time, the epoch if stat failed.
    pub mtime: SystemTime,
    pub marked: bool,
}

impl PlaylistEntry {
    /// Stats size and mtime; absolutizes lexically, no symlink resolution. A URL is kept
    /// verbatim and never stat'd.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if is_url(&path) {
            return Self {
                path,
                size: 0,
                mtime: SystemTime::UNIX_EPOCH,
                marked: false,
            };
        }
        let path = absolutize(&path);
        let meta = std::fs::metadata(&path).ok();
        Self {
            size: meta.as_ref().map_or(0, |m| m.len()),
            mtime: meta
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH),
            path,
            marked: false,
        }
    }

    pub fn display_name(&self) -> String {
        display_name(&self.path)
    }

    /// Lowercased display name, for case-insensitive sorting.
    pub fn sort_name(&self) -> String {
        self.display_name().to_lowercase()
    }
}
