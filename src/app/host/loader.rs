//! Decoding and preloading, off the event-loop thread.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use winit::event_loop::EventLoopProxy;

use crate::image::loader::{LoadError, load as load_image};
use crate::image::{Decoded, LoadOptions};
use crate::playlist::Playlist;
use crate::settings::Settings;

use crate::app::lock;

pub struct LoadResult {
    pub seq: u64,
    pub result: Result<Decoded, LoadError>,
}

/// One decode thread fed from a list that each new request replaces: whatever
/// has not started yet is dropped, so a burst of requests costs the decode
/// already running plus the newest one, not a thread and a decode each.
struct Worker<T> {
    shared: Arc<(Mutex<Queue<T>>, Condvar)>,
}

struct Queue<T> {
    pending: VecDeque<T>,
    busy: Option<T>,
    closed: bool,
}

impl<T: Clone + PartialEq + Send + 'static> Worker<T> {
    fn spawn(name: &str, mut run: impl FnMut(T) + Send + 'static) -> Self {
        let shared = Arc::new((
            Mutex::new(Queue::<T> {
                pending: VecDeque::new(),
                busy: None,
                closed: false,
            }),
            Condvar::new(),
        ));
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let (queue, wake) = &*thread_shared;
                loop {
                    let mut q = wake
                        .wait_while(lock(queue), |q| !q.closed && q.pending.is_empty())
                        .unwrap_or_else(PoisonError::into_inner);
                    if q.closed {
                        return;
                    }
                    let Some(job) = q.pending.pop_front() else {
                        continue;
                    };
                    q.busy = Some(job.clone());
                    drop(q); // release the lock while decoding
                    run(job);
                    lock(queue).busy = None;
                }
            })
            .unwrap_or_else(|err| panic!("cannot start the {name} thread: {err}"));
        Self { shared }
    }

    /// Drops everything not yet started and queues `jobs` instead.
    fn replace(&self, jobs: impl IntoIterator<Item = T>) {
        let (queue, wake) = &*self.shared;
        lock(queue).pending = jobs.into_iter().collect();
        wake.notify_one();
    }

    /// True while `job` is running, as opposed to queued.
    fn is_running(&self, job: &T) -> bool {
        lock(&self.shared.0).busy.as_ref() == Some(job)
    }
}

impl<T> Drop for Worker<T> {
    fn drop(&mut self) {
        lock(&self.shared.0).closed = true;
        self.shared.1.notify_all();
    }
}

/// Decodes, turning a decoder panic into an error so the spinner cannot hang.
fn decode_guarded(path: &Path, opts: &LoadOptions) -> Result<Decoded, LoadError> {
    catch_unwind(AssertUnwindSafe(|| load_image(path, opts)))
        .unwrap_or_else(|_| Err(LoadError::limits(path, "the decoder panicked")))
}

pub struct Loader {
    /// A decode of the current entry is in flight.
    pub loading: bool,
    /// Generation of the newest request; older results are stale.
    seq: u64,
    start: Instant,
    /// How long each spinner step lasts.
    tick: Duration,
    rx: Receiver<LoadResult>,
    /// Decode budgets.
    pub opts: LoadOptions,
    pub preload: usize,
    pub cache: HashMap<PathBuf, Decoded>,
    preload_rx: Receiver<(PathBuf, Decoded)>,
    /// The entry on screen; newest request wins.
    foreground: Worker<(u64, PathBuf)>,
    /// The entries ahead of it, newest window wins.
    background: Worker<PathBuf>,
}

impl Loader {
    pub fn new(settings: &Settings, wake: EventLoopProxy<()>) -> Self {
        let opts = settings.load.clone();
        let (tx, rx) = mpsc::channel();
        let (preload_tx, preload_rx) = mpsc::channel();

        let (fg_opts, fg_wake) = (opts.clone(), wake.clone());
        let foreground = Worker::spawn("decode", move |(seq, path): (u64, PathBuf)| {
            let result = decode_guarded(&path, &fg_opts);
            let _ = tx.send(LoadResult { seq, result });
            // The loop may be asleep; it has a result to pick up.
            let _ = fg_wake.send_event(());
        });
        let bg_opts = opts.clone();
        let background = Worker::spawn("preload", move |path: PathBuf| {
            match decode_guarded(&path, &bg_opts) {
                Ok(decoded) => {
                    let _ = preload_tx.send((path, decoded));
                    let _ = wake.send_event(());
                }
                Err(err) => tracing::debug!("preload skipped {}: {err}", path.display()),
            }
        });

        Self {
            loading: false,
            seq: 0,
            start: Instant::now(),
            tick: settings.loading_tick,
            rx,
            opts,
            preload: settings.preload,
            cache: HashMap::new(),
            preload_rx,
            foreground,
            background,
        }
    }

    /// Invalidates whatever is in flight; nothing is loading afterwards.
    pub fn cancel(&mut self) {
        self.seq = self.seq.wrapping_add(1);
        self.loading = false;
        self.foreground.replace([]);
    }

    /// Starts decoding `path` as the newest request.
    pub fn request(&mut self, path: PathBuf) {
        self.seq = self.seq.wrapping_add(1);
        self.loading = true;
        self.start = Instant::now();
        self.foreground.replace([(self.seq, path)]);
    }

    /// The next finished decode that is still the newest request.
    pub fn next_result(&mut self) -> Option<Result<Decoded, LoadError>> {
        while let Ok(res) = self.rx.try_recv() {
            if res.seq == self.seq {
                self.loading = false;
                return Some(res.result);
            }
        }
        None
    }

    /// Queues `paths` for preloading in place of the previous window. An entry
    /// already being decoded is left to finish, not queued twice.
    pub fn preload_paths(&mut self, paths: Vec<PathBuf>) {
        let queue: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| !self.background.is_running(p))
            .collect();
        if !queue.is_empty() {
            tracing::debug!("preloading {} image(s)", queue.len());
        }
        self.background.replace(queue);
    }

    /// Moves finished preloads into the cache, dropping ones that left the window.
    pub fn drain_preloads(&mut self, window: &[PathBuf]) {
        while let Ok((path, decoded)) = self.preload_rx.try_recv() {
            if window.contains(&path) {
                self.cache.insert(path, decoded);
            }
        }
    }

    /// The entries after the current one, wrapping around; `preload` of them.
    pub fn window(&self, playlist: &Playlist) -> Vec<PathBuf> {
        let len = playlist.len();
        if self.preload == 0 || len == 0 {
            return Vec::new();
        }
        let pos = playlist.position();
        // Past `len - 1` the window would only wrap back onto the current entry.
        (1..=self.preload.min(len - 1))
            .filter_map(|i| playlist.path_at((pos + i) % len))
            .collect()
    }

    /// The "loading..." spinner text, while a decode is in flight.
    pub fn status_text(&self) -> Option<String> {
        self.loading.then(|| {
            let step = (self.start.elapsed().as_millis() / self.tick.as_millis()) % 4;
            format!("loading{}", ".".repeat(step as usize))
        })
    }
}
