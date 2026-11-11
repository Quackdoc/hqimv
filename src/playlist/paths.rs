//! Paths and URLs: making them comparable, telling them apart, and naming them.

use std::path::{Component, Path, PathBuf};

/// Absolutizes lexically so watcher events compare equal.
pub fn absolutize(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(comp.as_os_str()),
        }
    }
    out
}

/// True when `path` names an `http://` or `https://` URL rather than a file.
pub fn is_url(path: &Path) -> bool {
    path.to_str().is_some_and(is_url_str)
}

/// String form of [`is_url`], for callers holding raw text.
pub fn is_url_str(value: &str) -> bool {
    let starts = |scheme: &str| {
        value
            .get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    };
    starts("http://") || starts("https://")
}

/// A URL with any query or fragment removed.
fn url_path(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    &url[..end]
}

/// What to call `path` on screen: a URL's last segment, else the file name,
/// else the whole path (such as `/`).
pub fn display_name(path: &Path) -> String {
    match path.to_str() {
        Some(url) if is_url_str(url) => url_name(url),
        _ => path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

/// Last path segment of a URL, or the whole URL when the path has none.
fn url_name(url: &str) -> String {
    url_path(url)
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(url)
        .to_string()
}

/// Directory part of a URL, for `$HQIMV_DIR`.
pub fn url_dir(url: &str) -> String {
    // Only a `/` after the host counts; the ones in `://` are not a path.
    let host = url.find("://").map_or(0, |i| i + 3);
    match url_path(url).rfind('/') {
        Some(cut) if cut > host => url[..cut].to_string(),
        _ => url.to_string(),
    }
}
