//! Building a playlist by reading a directory.

use std::io;
use std::path::Path;
use std::path::PathBuf;

use super::extensions::path_matches_extensions;
use super::{Playlist, PlaylistEntry, SortKey};

#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    pub recursive: bool,
    pub sort: SortKey,
    /// Flips the sorted order.
    pub reverse: bool,
    pub extensions: Vec<String>,
}

impl Playlist {
    /// Empty directory yields an empty playlist, not an error.
    pub fn scan_dir(dir: &Path, opts: &ScanOptions) -> io::Result<Self> {
        let mut entries: Vec<PlaylistEntry> = Vec::new();

        let mut stack = vec![dir.to_path_buf()];
        let mut visited_dirs = 0usize;
        while let Some(current) = stack.pop() {
            visited_dirs += 1;
            let read = match std::fs::read_dir(&current) {
                Ok(read) => read,
                Err(err) if opts.recursive => {
                    tracing::warn!("skipping {}: {err}", current.display());
                    continue;
                }
                Err(err) => return Err(err),
            };

            let mut subdirs: Vec<PathBuf> = Vec::new();
            for entry in read.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    if opts.recursive {
                        subdirs.push(path);
                    }
                    continue;
                }
                // Regular files only: a FIFO or device named `x.png` would
                // block the decoder forever. A symlink counts if it leads to one.
                let regular = file_type.is_file()
                    || (file_type.is_symlink()
                        && std::fs::metadata(&path).is_ok_and(|m| m.is_file()));
                if !regular {
                    continue;
                }
                if !path_matches_extensions(&path, &opts.extensions) {
                    continue;
                }
                entries.push(PlaylistEntry::new(path));
            }
            // The stack pops last-in first, so push in reverse to visit
            // subdirectories in the order they were found.
            stack.extend(subdirs.into_iter().rev());
        }

        tracing::debug!(
            "scanned {visited_dirs} directories, found {} images",
            entries.len()
        );

        opts.sort.sort(&mut entries, opts.reverse);

        Ok(Self::from_entries(entries))
    }
}
