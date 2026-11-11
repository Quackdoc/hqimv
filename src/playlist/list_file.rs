//! The playlist as a text file: one path per line.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use super::Playlist;

impl Playlist {
    /// Paths, one per line, in playback order. Raw bytes, so a name that is not
    /// UTF-8 survives. A path a line cannot hold (a newline, a leading `#`, or
    /// edge whitespace that reading would trim) is skipped with a warning.
    pub fn save_lines(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in self.entries() {
            let bytes = entry.path.as_os_str().as_bytes();
            let edge_space = bytes.first().is_some_and(u8::is_ascii_whitespace)
                || bytes.last().is_some_and(u8::is_ascii_whitespace);
            if bytes.is_empty()
                || bytes.contains(&b'\n')
                || bytes.contains(&b'\r')
                || bytes[0] == b'#'
                || edge_space
            {
                tracing::warn!(
                    "not saving {}: a playlist line cannot hold that name",
                    entry.path.display()
                );
                continue;
            }
            out.extend_from_slice(bytes);
            out.push(b'\n');
        }
        out
    }
}

/// One path per line; blank lines and `#` comments are skipped, and edge
/// whitespace (including a CR from CRLF line ends) is trimmed. Works on bytes
/// so a name that is not UTF-8 is kept.
pub fn parse_path_list(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|&b| b == b'\n')
        .map(<[u8]>::trim_ascii)
        .filter(|line| !line.is_empty() && line[0] != b'#')
        .map(|line| PathBuf::from(OsStr::from_bytes(line)))
        .collect()
}

/// Trimmed line, or `None` if blank or a `#` comment.
pub fn is_content_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        None
    } else {
        Some(line)
    }
}
