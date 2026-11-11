//! Keeping the playlist in step with the directories it was read from.

use std::collections::HashMap;
use std::path::PathBuf;

use super::{App, FsEvent};
use crate::playlist::{is_url, path_matches_extensions};

impl App {
    /// Applies the watcher's events to the playlist. If the current entry
    /// vanished, loads whatever is current now.
    pub fn drain_fs_events(&mut self) {
        // Last event per path wins, so `rm x; cp new x` is a change to `x`
        // rather than a removal that follows the addition.
        let mut latest: HashMap<PathBuf, bool> = HashMap::new();
        let mut dirs_gone: Vec<PathBuf> = Vec::new();
        let mut overflow = false;
        while let Ok(ev) = self.host.fs.rx.try_recv() {
            match ev {
                FsEvent::Removed(p) => {
                    latest.insert(p, false);
                }
                FsEvent::Added(p) => {
                    latest.insert(p, true);
                }
                FsEvent::DirGone(dir) => dirs_gone.push(dir),
                FsEvent::Overflow => overflow = true,
            }
        }
        // A deleted directory takes everything under it, whether or not each
        // file got an event of its own.
        for dir in dirs_gone {
            for p in self
                .session
                .playlist
                .paths()
                .filter(|p| p.starts_with(&dir))
            {
                latest.insert(p.clone(), false);
            }
            self.host.fs.watched_dirs.remove(&dir);
        }
        if overflow {
            tracing::warn!("too many file changes at once; rechecking the watched directories");
            self.rescan_into(&mut latest);
        }

        let mut gone: Vec<PathBuf> = Vec::new();
        let mut arrived: Vec<PathBuf> = Vec::new();
        let mut changed: Vec<PathBuf> = Vec::new();
        for (p, present) in latest {
            match (present, self.session.playlist.contains(&p)) {
                (false, _) => gone.push(p),
                (true, true) => changed.push(p),
                (true, false) => arrived.push(p),
            }
        }

        let cur = self.session.playlist.current_owned();
        // Rewritten in place: whatever was decoded from it is out of date.
        let mut reload = false;
        for p in &changed {
            self.host.loader.cache.remove(p);
            reload |= cur.as_ref() == Some(p);
        }
        if !arrived.is_empty() {
            self.absorb_added_paths(arrived);
        }
        let mut removed: Vec<PathBuf> = Vec::new();
        for p in gone {
            if self.session.playlist.remove_path(&p) {
                tracing::info!("removed {} (deleted on disk)", p.display());
                removed.push(p);
            }
        }
        if !removed.is_empty() {
            self.notify(Self::list_msg("removed", "from", &removed));
            reload |= cur.is_some_and(|c| !self.session.playlist.paths().any(|q| q == &c));
        }
        if reload {
            self.load_current_async();
        } else if !removed.is_empty() {
            self.refresh();
        }
    }

    /// After the kernel dropped events: marks every playlist file that is gone,
    /// and every supported file in a watched directory that is not listed yet.
    fn rescan_into(&self, latest: &mut HashMap<PathBuf, bool>) {
        for p in self.session.playlist.paths() {
            if !is_url(p) && !p.exists() {
                latest.insert(p.clone(), false);
            }
        }
        let extensions = &self.setup.settings.scan.extensions;
        for dir in &self.host.fs.watched_dirs {
            let Ok(read) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                let regular = entry.file_type().is_ok_and(|t| t.is_file())
                    || std::fs::metadata(&path).is_ok_and(|m| m.is_file());
                if regular
                    && path_matches_extensions(&path, extensions)
                    && !self.session.playlist.contains(&path)
                {
                    latest.insert(path, true);
                }
            }
        }
    }
}
