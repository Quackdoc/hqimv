//! One inotify thread watching the directories the playlist's files live in.
//! The app drains events per frame, keeping the last per path. `CREATE` is ignored so half-written
//! files are never picked up.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use super::lock;

use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask, Watches};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum FsEvent {
    Removed(PathBuf),
    Added(PathBuf),
    /// A watched directory was deleted or moved away.
    DirGone(PathBuf),
    /// The kernel dropped events; whatever changed since is unknown.
    Overflow,
}

pub fn dir_of(path: &Path) -> PathBuf {
    path.parent().unwrap_or(Path::new("/")).to_path_buf()
}

const MASK: WatchMask = WatchMask::DELETE
    .union(WatchMask::MOVED_FROM)
    .union(WatchMask::MOVED_TO)
    .union(WatchMask::CLOSE_WRITE)
    .union(WatchMask::DELETE_SELF)
    .union(WatchMask::MOVE_SELF);

type Dirs = Arc<Mutex<HashMap<WatchDescriptor, PathBuf>>>;

/// Handle for adding directories to the watch thread.
pub struct Watcher {
    watches: Watches,
    dirs: Dirs,
}

impl Watcher {
    /// Starts watching `dir`; the failure, if any, is reported here.
    pub fn add(&mut self, dir: &Path) {
        // Held across the add, so the thread cannot see an event for the new
        // watch before it knows which directory that is.
        let mut dirs = lock(&self.dirs);
        match self.watches.add(dir, MASK) {
            Ok(wd) => {
                dirs.insert(wd, dir.to_path_buf());
            }
            Err(err) => tracing::warn!(
                "cannot watch {}: {err} (changes there will not be noticed; the \
                 fs.inotify.max_user_watches limit may be too low)",
                dir.display()
            ),
        }
    }
}

/// Starts the thread; `None` if inotify is unavailable.
pub fn start(tx: Sender<FsEvent>, wake: EventLoopProxy<()>) -> Option<Watcher> {
    let inotify = match Inotify::init() {
        Ok(it) => it,
        Err(err) => {
            tracing::warn!("file watcher off: {err}");
            return None;
        }
    };
    let watcher = Watcher {
        watches: inotify.watches(),
        dirs: Arc::new(Mutex::new(HashMap::new())),
    };
    let dirs = Arc::clone(&watcher.dirs);
    let mut watches = watcher.watches.clone();
    let spawned = std::thread::Builder::new()
        .name("watch".into())
        .spawn(move || run(inotify, &dirs, &mut watches, &tx, &wake));
    if let Err(err) = spawned {
        tracing::warn!("file watcher off: {err}");
        return None;
    }
    Some(watcher)
}

fn run(
    mut inotify: Inotify,
    dirs: &Dirs,
    watches: &mut Watches,
    tx: &Sender<FsEvent>,
    wake: &EventLoopProxy<()>,
) {
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        let events = match inotify.read_events_blocking(&mut buffer) {
            Ok(events) => events,
            Err(err) => {
                tracing::warn!("file watcher stopped: {err}");
                return;
            }
        };
        // Queue first, ping once: one wakeup per batch, not per file.
        let mut batch = Vec::new();
        for event in events {
            if event.mask.contains(EventMask::Q_OVERFLOW) {
                batch.push(FsEvent::Overflow);
                continue;
            }
            let dir = lock(dirs).get(&event.wd).cloned();
            let Some(dir) = dir else { continue };

            // The kernel is done with a watch it ignores.
            if event.mask.contains(EventMask::IGNORED) {
                lock(dirs).remove(&event.wd);
                continue;
            }
            if event
                .mask
                .intersects(EventMask::DELETE_SELF | EventMask::MOVE_SELF)
            {
                // A moved directory is still watched under its new name; drop
                // that, since the playlist's paths no longer lead there.
                let _ = watches.remove(event.wd.clone());
                lock(dirs).remove(&event.wd);
                batch.push(FsEvent::DirGone(dir));
                continue;
            }
            let Some(name) = event.name else { continue };
            let abs = dir.join(name);
            if event
                .mask
                .intersects(EventMask::DELETE | EventMask::MOVED_FROM)
            {
                batch.push(FsEvent::Removed(abs));
            } else if event
                .mask
                .intersects(EventMask::MOVED_TO | EventMask::CLOSE_WRITE)
            {
                batch.push(FsEvent::Added(abs));
            }
        }
        let queued = !batch.is_empty();
        if batch.into_iter().any(|ev| tx.send(ev).is_err()) {
            return;
        }
        if queued && wake.send_event(()).is_err() {
            return;
        }
    }
}
