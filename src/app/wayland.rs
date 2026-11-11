//! Window creation and `wl_shm` binding; the surface stays owned by winit.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use smithay_client_toolkit::shm::Shm;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use super::App;
use super::error::AppError;
use super::host::{Presentation, Wl};
use crate::present::{self, Presenter};

/// What the queue's handlers dispatch into; kept for the life of the window.
pub struct WlState {
    pub shm: Shm,
}

impl smithay_client_toolkit::shm::ShmHandler for WlState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl wayland_client::Dispatch<WlRegistry, GlobalListContents> for WlState {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: wayland_client::protocol::wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qh: &wayland_client::QueueHandle<Self>,
    ) {
        // Globals are bound explicitly, so late arrivals need nothing.
    }
}

// Bridges SCTK's `Dispatch2` to the `Dispatch` the proxy needs.
smithay_client_toolkit::delegate_dispatch2!(WlState);

impl App {
    pub fn window_created(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.screen.wl.is_some() {
            return;
        }

        // Decode before sizing the window
        self.load_current();
        // A lone file that will not load leaves nothing to show or step to.
        if self.session.playlist.len() == 1
            && self.session.content.frame().is_none()
            && !self.setup.cli.print_format
        {
            self.fail(event_loop, &AppError::NoImages);
            return;
        }

        let mut attrs = winit::window::WindowAttributes::default()
            .with_title("hqimv")
            .with_visible(!self.setup.cli.hidden)
            .with_decorations(self.setup.settings.decorations)
            // Never set an opaque region: the image's alpha is presented
            // directly, and the background can be translucent too.
            .with_transparent(true);
        let size = self
            .setup
            .settings
            .window_size
            .or_else(|| self.session.content.frame().map(|i| i.size()));
        if let Some((w, h)) = size {
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(f64::from(w), f64::from(h)));
        }

        let window = match event_loop.create_window(attrs) {
            Ok(window) => window,
            Err(err) => {
                self.fail(event_loop, &err.into());
                return;
            }
        };
        if self.setup.settings.fullscreen {
            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        let presentation = match self.init_wayland(&window) {
            Ok(presentation) => presentation,
            Err(err) => {
                self.fail(event_loop, &err);
                return;
            }
        };
        self.host.screen.wl = Some(Wl {
            window,
            presentation,
            pending_resize: None,
        });
        if self.setup.cli.print_format {
            event_loop.exit();
        }
        self.invalidate();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: &AppError) {
        tracing::error!("{err}");
        self.failed = true;
        event_loop.exit();
    }

    fn init_wayland(&mut self, window: &Window) -> Result<Presentation, AppError> {
        let dh = window.display_handle()?.as_raw();
        let raw_window_handle::RawDisplayHandle::Wayland(dh) = dh else {
            return Err(AppError::NotWayland);
        };

        // SAFETY: `dh.display` is winit's `wl_display`; "guest" mode keeps
        // ownership with winit and never closes it on drop.
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(dh.display.as_ptr().cast())
        };
        let conn = wayland_client::Connection::from_backend(backend);

        // One queue only
        let (globals, mut queue) = wayland_client::globals::registry_queue_init::<WlState>(&conn)?;
        let qh = queue.handle();
        let shm = Shm::bind(&globals, &qh)?;

        let mut state = WlState { shm };
        queue.roundtrip(&mut state)?;
        let advertised = state.shm.formats().to_vec();
        tracing::debug!("compositor advertises {advertised:?}");

        // Preference only: a compositor with no alpha format still works, it
        // just cannot show anything through the window. Wanted for a translucent
        // background even when this image is opaque, since the margins around it
        // (and every later image) are drawn with that background.
        let wants_alpha = self.session.view.background[3] < 1.0
            || self.session.content.frame().is_some_and(|i| !i.is_opaque());

        let format = match self.setup.settings.format.as_deref() {
            Some(name) => present::format::by_name(name)
                .ok_or_else(|| AppError::UnknownFormat(name.to_string()))?,
            None => present::negotiate(&advertised, wants_alpha).ok_or(AppError::NoFormat)?,
        };

        if self.setup.cli.print_format {
            super::out(format!("{format:?}\nadvertised: {advertised:?}\n"));
        }

        tracing::info!(
            "presenting in {} ({} bits per channel{})",
            format.name,
            format.depth,
            if format.is_float { ", float" } else { "" }
        );

        let (width, height) = super::render::window_px(window);
        let presenter = Presenter::new(
            &state.shm,
            format,
            width,
            height,
            self.session.view.background,
        )?;

        let wh = window.window_handle()?.as_raw();
        let raw_window_handle::RawWindowHandle::Wayland(wh) = wh else {
            return Err(AppError::NotWayland);
        };
        let surface = present::wrap_surface(&conn, wh.surface).ok_or(AppError::NoSurface)?;

        Ok(Presentation {
            surface,
            presenter,
            state,
            conn,
            queue,
        })
    }

    /// Never reads the socket: winit owns reads, and reading here races it.
    pub fn pump_wayland(&mut self) {
        let Some(presentation) = self.host.screen.wl.as_mut().map(|wl| &mut wl.presentation) else {
            return;
        };
        let Presentation { queue, state, .. } = presentation;
        if let Err(err) = queue.dispatch_pending(state) {
            tracing::debug!("wayland dispatch failed: {err}");
        }
    }
}
