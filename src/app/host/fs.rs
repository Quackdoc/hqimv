//! Watching the playlist's directories for files appearing and disappearing.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use winit::event_loop::EventLoopProxy;

use crate::playlist::{Playlist, is_url};

use crate::app::watch::{self, FsEvent, Watcher};

/// Filesystem watch: the event channel, and the directories already watched.
pub struct Fs {
    pub rx: Receiver<FsEvent>,
    // Directories already watched; avoids double-watching runtime additions.
    pub watched_dirs: HashSet<PathBuf>,
    watcher: Option<Watcher>,
}

impl Fs {
    /// Watches the directory of every local file in `playlist`.
    pub fn new(playlist: &Playlist, wake: EventLoopProxy<()>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut fs = Self {
            rx,
            watched_dirs: HashSet::new(),
            watcher: watch::start(tx, wake),
        };
        for path in playlist.paths().filter(|p| !is_url(p)) {
            fs.watch(watch::dir_of(path));
        }
        fs
    }

    /// Starts watching `dir` unless it already is.
    pub fn watch(&mut self, dir: PathBuf) {
        if let Some(watcher) = &mut self.watcher
            && self.watched_dirs.insert(dir.clone())
        {
            watcher.add(&dir);
        }
    }
}
