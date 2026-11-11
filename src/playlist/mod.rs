//! Ordered image list; paths, not pixels.

mod entry;
mod extensions;
mod list_file;
mod navigate;
pub mod order;
mod paths;
mod scan;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use entry::PlaylistEntry;
pub use extensions::{default_extensions, parse_extensions, path_matches_extensions};
pub use list_file::{is_content_line, parse_path_list};
pub use navigate::{EndAction, Step};
pub use order::SortKey;
pub use paths::{absolutize, display_name, is_url, is_url_str, url_dir};
pub use scan::ScanOptions;

#[derive(Debug, Clone, Default)]
pub struct Playlist {
    entries: Vec<PlaylistEntry>,
    /// Index into `entries`.
    current: usize,
    /// Paths visited before the current one, newest last.
    history: Vec<PathBuf>,
    /// Every entry's path, so `contains` is a lookup rather than a scan.
    known: HashSet<PathBuf>,
}

impl Playlist {
    /// Preserves the given order; later entries repeating a path are dropped.
    pub fn from_entries(mut entries: Vec<PlaylistEntry>) -> Self {
        let mut known = HashSet::with_capacity(entries.len());
        entries.retain(|e| known.insert(e.path.clone()));
        Self {
            entries,
            current: 0,
            history: Vec::new(),
            known,
        }
    }

    pub fn into_entries(self) -> Vec<PlaylistEntry> {
        self.entries
    }

    /// Capped so holding an arrow key cannot grow history without bound.
    pub fn push_history(&mut self, path: PathBuf) {
        const HISTORY_MAX: usize = 128;
        self.history.push(path);
        if self.history.len() > HISTORY_MAX {
            let excess = self.history.len() - HISTORY_MAX;
            self.history.drain(..excess);
        }
    }

    /// The path visited before the current position, if any is left.
    pub fn pop_history(&mut self) -> Option<PathBuf> {
        self.history.pop()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn position(&self) -> usize {
        self.current
    }

    pub fn current(&self) -> Option<&PlaylistEntry> {
        self.entries.get(self.current)
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current().map(|e| e.path.as_path())
    }

    pub fn current_owned(&self) -> Option<PathBuf> {
        self.current_path().map(Path::to_path_buf)
    }

    pub fn marked_count(&self) -> usize {
        self.entries.iter().filter(|e| e.marked).count()
    }

    pub fn entries(&self) -> impl Iterator<Item = &PlaylistEntry> {
        self.entries.iter()
    }

    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.entries.iter().map(|e| &e.path)
    }

    pub fn path_at(&self, pos: usize) -> Option<PathBuf> {
        self.entries.get(pos).map(|e| e.path.clone())
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.known.contains(path)
    }

    /// Appends; keeps current position. Duplicates are skipped, compared
    /// after absolutizing, as the entries themselves are stored.
    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            let path = if is_url(&path) {
                path
            } else {
                absolutize(&path)
            };
            if self.known.insert(path.clone()) {
                self.entries.push(PlaylistEntry::new(path));
            }
        }
    }

    pub fn toggle_mark(&mut self) -> bool {
        let Some(entry) = self.entries.get_mut(self.current) else {
            return false;
        };
        entry.marked = !entry.marked;
        entry.marked
    }

    pub fn mark_all(&mut self) {
        for entry in &mut self.entries {
            entry.marked = true;
        }
    }

    pub fn unmark_all(&mut self) {
        for entry in &mut self.entries {
            entry.marked = false;
        }
    }

    /// Removes path; stays on the following entry.
    pub fn remove_path(&mut self, path: &Path) -> bool {
        let Some(index) = self.entries.iter().position(|e| e.path == path) else {
            return false;
        };
        self.entries.remove(index);
        self.known.remove(path);
        // Shifting down keeps the current entry unless it was removed, in which
        // case the following one takes its place.
        if index < self.current {
            self.current -= 1;
        }
        self.current = self.current.min(self.entries.len().saturating_sub(1));
        true
    }

    pub fn remove_current(&mut self) -> Option<PathBuf> {
        let path = self.current_owned()?;
        self.remove_path(&path).then_some(path)
    }

    /// Removes the entry at `pos` without moving off the current one, unless
    /// that is the one removed.
    pub fn remove_at(&mut self, pos: usize) -> Option<PathBuf> {
        let path = self.path_at(pos)?;
        self.remove_path(&path).then_some(path)
    }

    pub fn remove_by_name(&mut self, name: &str) -> bool {
        let found = self.entries.iter().position(|e| e.display_name() == name);
        found.and_then(|i| self.remove_at(i)).is_some()
    }
}
