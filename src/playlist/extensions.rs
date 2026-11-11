//! Which file extensions the playlist accepts.

use std::path::Path;

use super::paths::is_url;

/// Extensions the decoders in this build accept.
pub fn default_extensions() -> Vec<String> {
    [
        "jpg", "jpeg", "jfif", "png", "apng", "gif", "webp", "tif", "tiff", "tga", "bmp", "ico",
        "hdr", "exr", "pbm", "pam", "ppm", "pgm", "pnm", "qoi", "jxl",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Empty `extensions` means any file. A URL is always accepted: its extension is
/// advisory, and it is only ever requested explicitly, so decode failure is the
/// better filter.
pub fn path_matches_extensions(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() || is_url(path) {
        return true;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    extensions.contains(&ext)
}

/// Splits on `,; +`; lowercases, strips leading dots.
pub fn parse_extensions(spec: &str) -> Vec<String> {
    spec.split([',', ';', ' ', '+'])
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}
