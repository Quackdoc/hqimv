//! The viewer: the winit application, and the composition root for its parts.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::WindowId;

use crate::cli::Cli;
use crate::config::Config;
use crate::settings::Settings;

mod actions;
mod error;
mod fs_events;
mod host;
mod input;
mod playlist_build;
mod render;
mod session;
mod setup;
mod signals;
mod watch;
mod wayland;

use host::{Commands, Fs, Host, Loader, Screen};
use input::Input;
use playlist_build::build_playlist;
use session::Session;
use setup::Setup;

pub use error::AppError;
pub use watch::FsEvent;

/// Writes to stdout, ignoring errors: a closed pipe must not take the viewer down
/// the way `println!` would.
pub fn out(bytes: impl AsRef<[u8]>) {
    let _ = std::io::stdout().write_all(bytes.as_ref());
}

/// Locks `m`, ignoring poisoning: a panicked worker must not take the viewer down.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct App {
    setup: Setup,

    session: Session,
    host: Host,

    // Set by `quit`, honoured in `about_to_wait`.
    exit_requested: bool,
    // Set by a termination signal; turned into `quit` in `about_to_wait`.
    shutdown: Arc<AtomicBool>,
    // The viewer stopped because it could not start, not because it was closed.
    failed: bool,
}

impl App {
    pub fn new(cli: Cli, config: Config, wake: EventLoopProxy<()>) -> Result<Self, AppError> {
        let bindings = config.build_bindings(&cli.bind_key)?;

        let settings = Settings::resolve(&cli, &config);
        let playlist = build_playlist(&cli, &config, &settings.scan)?;
        if playlist.is_empty() && !cli.print_format {
            return Err(AppError::NoImages);
        }

        let (command_tx, command_rx) = mpsc::channel();

        let session = Session::new(playlist, &settings, cli.slideshow);
        let host = Host {
            input: Input {
                bindings,
                modifiers: Default::default(),
            },
            loader: Loader::new(&settings, wake.clone()),
            screen: Screen::new(cli.hidden),
            fs: Fs::new(&session.playlist, wake.clone()),
            commands: Commands {
                rx: command_rx,
                tx: command_tx,
            },
        };

        Ok(Self {
            setup: Setup { cli, settings },
            session,
            host,
            exit_requested: false,
            shutdown: signals::watch(wake),
            failed: false,
        })
    }

    /// True if the event loop ended because startup failed.
    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn invalidate(&mut self) {
        self.host.screen.dirty = true;
        // Frames go through RedrawRequested so `pre_present_notify` runs.
        if let Some(wl) = self.host.screen.wl.as_ref() {
            wl.window.request_redraw();
        }
    }

    /// Retitles and repaints. Almost every state change needs both, and the
    /// title goes first so it never names an image that is not on screen.
    pub fn refresh(&mut self) {
        self.session.chrome.pending = !self.update_title();
        self.invalidate();
    }

    /// Shows the next animation frame once the one on screen is due.
    fn advance_animation(&mut self) {
        if self.session.content.advance() {
            self.invalidate();
        }
    }

    pub fn window_title(&self) -> String {
        let Some(entry) = self
            .session
            .playlist
            .current()
            .filter(|_| self.session.view.status_line)
        else {
            return "hqimv".to_string();
        };
        let total = self.session.playlist.len();
        let pos = self.session.playlist.position() + 1;
        let mark = if entry.marked { "*" } else { "" };
        format!("{mark}{} [{pos}/{total}] - hqimv", entry.display_name())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window_created(event_loop);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.resize();
            }
            WindowEvent::ModifiersChanged(mods) => self.host.input.modifiers = mods,
            WindowEvent::KeyboardInput { event, .. } => self.on_key(event),
            WindowEvent::MouseWheel { delta, .. } => self.on_scroll(delta),
            WindowEvent::RedrawRequested => self.redraw(),
            other => {
                tracing::debug!("winit event: {other:?}");
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.pump_wayland();
        self.drain_commands();
        self.drain_fs_events();
        self.drain_loads();
        self.drain_preloads();
        self.advance_animation();

        // A step while a decode is in flight would only pile up work; skip it.
        if self.session.slideshow.due() && !self.host.loader.loading {
            self.step_relative(1);
        }

        // Re-sent while a change is owed: a single `set_title` sometimes never
        // lands, so retry until it does.
        if self.session.chrome.pending && self.update_title() {
            self.session.chrome.pending = false;
        }
        let title_settled = !self.session.chrome.pending;

        if self.shutdown.load(Ordering::SeqCst) {
            self.quit();
        }
        if self.exit_requested {
            event_loop.exit();
            return;
        }

        let mut next = self
            .session
            .slideshow
            .deadline()
            .map_or(ControlFlow::Wait, ControlFlow::WaitUntil);

        // An unfinished key sequence needs a wakeup to time out, and is dropped
        // once it has.
        self.host.input.bindings.expire();
        if self.host.input.bindings.has_pending()
            && let ControlFlow::Wait = next
        {
            next = ControlFlow::WaitUntil(Instant::now() + self.setup.settings.play_tick);
        }

        // The spinner and the notice need periodic wakeups; the spinner runs
        // off the wall clock.
        if self
            .session
            .chrome
            .notice
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() >= self.setup.settings.notice_ttl)
        {
            self.session.chrome.notice = None;
            self.invalidate();
        }
        if self.host.loader.loading || self.session.chrome.notice.is_some() {
            // Repaint only once the animation has actually advanced. Forcing a
            // frame every iteration spins the loop against the compositor.
            let now = Instant::now();
            if now.duration_since(self.host.screen.last_anim) >= self.setup.settings.loading_tick {
                self.host.screen.last_anim = now;
                self.invalidate();
            }
            let wake = self.host.screen.last_anim + self.setup.settings.loading_tick;
            next = earlier(next, wake);
        }

        // An owed frame comes back on its own, without waiting for an event.
        if let Some(at) = self.host.screen.retry_at.take() {
            next = earlier(next, at);
        }

        // A title change that has not been delivered yet: come back and retry.
        if !title_settled && self.host.screen.wl.is_some() {
            let wake = Instant::now() + self.setup.settings.present_retry;
            next = earlier(next, wake);
        }

        // An animation needs a wakeup for the frame that comes due next.
        if let Some(due) = self.session.content.next_frame_at() {
            next = earlier(next, due);
        }

        event_loop.set_control_flow(next);
    }
}

/// Deadline merge for `about_to_wait`: whatever is already pending stands
/// unless `at` is sooner, in which case that one wins.
fn earlier(next: ControlFlow, at: Instant) -> ControlFlow {
    match next {
        ControlFlow::WaitUntil(d) => ControlFlow::WaitUntil(d.min(at)),
        _ => ControlFlow::WaitUntil(at),
    }
}
