//! Decoding an image, fitting it to the window, and handing pixels to the compositor.

use super::App;
use super::host::flush;
use std::borrow::Cow;
use std::path::PathBuf;

use crate::image::loader::{LoadError, load as load_image};
use crate::image::overlay;
use crate::image::resample::{output_size, resize, sane_factor, scaled_len};
use crate::image::{Decoded, WorkingImage};
use crate::playlist::display_name;
use winit::window::Window;

// Minimised windows report 0x0, which would make a zero-sized buffer.
pub fn window_px(window: &Window) -> (u32, u32) {
    let size = window.inner_size();
    (size.width.max(1), size.height.max(1))
}

impl App {
    /// Puts a decode outcome on screen; a failure leaves the viewer empty.
    fn install(&mut self, result: Result<Decoded, LoadError>) {
        match result {
            Ok(decoded) => {
                self.session.content.set(decoded);
                if let Some(path) = self.session.playlist.current_path() {
                    tracing::info!("loaded {}", path.display());
                }
            }
            Err(err) => {
                // A broken file must not take the viewer down.
                tracing::warn!("{err}");
                self.session.content.clear();
                // The log is not where you are looking; say so on screen too.
                if let Some(path) = self.session.playlist.current_path() {
                    let name = display_name(path);
                    self.notify(format!("failed to load {name}"));
                }
            }
        }
    }

    pub fn load_current(&mut self) {
        let Some(path) = self.session.playlist.current_owned() else {
            self.session.content.clear();
            self.refresh();
            return;
        };

        let result = load_image(&path, &self.host.loader.opts);
        self.install(result);
        // Retitle after the swap so it never names an image still decoding.
        self.refresh();
    }

    /// Old image stays up until `drain_loads` installs the result.
    pub fn load_current_async(&mut self) {
        let Some(path) = self.session.playlist.current_owned() else {
            // A decode already in flight must not land on an empty playlist.
            self.host.loader.cancel();
            self.session.content.clear();
            self.refresh();
            return;
        };

        // A preload that finished while the loop slept is not in the cache yet.
        self.drain_preloads();
        if let Some(decoded) = self.host.loader.cache.remove(&path) {
            tracing::debug!("load_current_async: preload hit");
            // Still bumps the sequence, so an older decode goes stale.
            self.host.loader.cancel();
            self.install(Ok(decoded));
            self.refresh();
            self.kick_preload();
            return;
        }
        self.host.loader.request(path);
        tracing::debug!("load_current_async: decode queued");
        // Retitle now so the title tracks the selection through slow decodes.
        self.refresh();
    }

    /// Stale generations are dropped.
    pub fn drain_loads(&mut self) {
        while let Some(result) = self.host.loader.next_result() {
            self.install(result);
            // The name was held back while decoding; show it for the full time now.
            self.notify_current_if_shown();
            self.refresh();
            self.kick_preload();
        }
    }

    pub fn kick_preload(&mut self) {
        if self.host.loader.preload == 0 {
            self.host.loader.cache.clear();
            return;
        }
        let window = self.host.loader.window(&self.session.playlist);
        self.host.loader.cache.retain(|k, _| window.contains(k));
        let missing: Vec<PathBuf> = window
            .into_iter()
            .filter(|p| !self.host.loader.cache.contains_key(p))
            .collect();
        self.host.loader.preload_paths(missing);
    }

    pub fn drain_preloads(&mut self) {
        let window = self.host.loader.window(&self.session.playlist);
        self.host.loader.drain_preloads(&window);
    }

    fn output_geometry(&self) -> Option<(u32, u32)> {
        let image = self.session.content.frame()?;
        let wl = self.host.screen.wl.as_ref()?;
        let (w, h) = window_px(&wl.window);
        let (w, h) = output_size(
            self.session.view.scale_mode,
            image.size(),
            (w, h),
            wl.window.scale_factor(),
        );
        let z = sane_factor(self.session.view.zoom);
        Some((scaled_len(w, z), scaled_len(h, z)))
    }

    /// True when the loop still owes a frame.
    pub fn draw(&mut self) -> bool {
        // Rebuild the buffer pool here, once per frame, rather than once per
        // `Resized` event. A failure leaves the presenter with no buffers, which
        // reads as a retry below and is retried on the next frame.
        if let Some(wl) = self.host.screen.wl.as_mut()
            && let Some((w, h)) = wl.pending_resize.take()
            && let Err(err) = wl
                .presentation
                .presenter
                .resize(&wl.presentation.state.shm, w, h)
        {
            tracing::warn!("cannot resize the buffer pool: {err}");
        }

        let blank;
        let (image, (w, h)) = match self.session.content.frame() {
            Some(image) => {
                let Some(geometry) = self.output_geometry() else {
                    return false;
                };
                (image, geometry)
            }
            None => {
                // Nothing decoded (a failed load, or an empty playlist). Still
                // repaint, so the old image and spinner do not linger and the
                // notice can show: one transparent pixel over the background.
                if self.host.screen.wl.is_none() {
                    return false;
                }
                blank = WorkingImage::blank();
                (&blank, (1, 1))
            }
        };
        // Owned up front: the presenter borrow below holds `self.host`.
        let info_text = (self.session.view.info_overlay && self.session.content.frame().is_some())
            .then(|| {
                format!(
                    "{}x{} {}-bit {:.2}x {:?}",
                    image.width(),
                    image.height(),
                    image.source_bit_depth().unwrap_or(0),
                    self.session.view.zoom,
                    self.session.view.scale_mode
                )
            });
        let window_h = self
            .host
            .screen
            .wl
            .as_ref()
            .map_or(0, |wl| window_px(&wl.window).1);

        let scaled = if image.size() == (w, h) && image.is_opaque() {
            // Already what would be presented: borrow rather than pay a
            // full-size clone every frame.
            Cow::Borrowed(image)
        } else {
            let sized = if image.size() == (w, h) {
                image.clone()
            } else {
                resize(image, w, h, self.session.view.filter)
            };
            Cow::Owned(sized.composite_over(self.session.view.background))
        };

        let (px, margin) = (
            self.setup.settings.overlay_px,
            self.setup.settings.overlay_margin,
        );
        let line_at = |text: &str, y: i32| overlay::TextOverlay::new(text, px, margin, y);
        let mut overlays = Vec::new();
        let mut oy = margin;
        let label = self.current_label();
        if let Some(text) = self.host.loader.status_text() {
            let line = line_at(&text, oy);
            oy += line.size().1 as i32 + 8;
            overlays.push(line);
        }
        // The navigation notice waits for the decode, then takes the place of
        // "loading..." instead of stacking under it.
        if let Some((msg, _)) = self
            .session
            .chrome
            .notice
            .as_ref()
            .filter(|(msg, _)| !(self.host.loader.loading && label.as_deref() == Some(msg)))
        {
            overlays.push(line_at(msg, oy));
        }
        if self.session.view.info_overlay
            && let Some(text) = info_text.as_ref()
        {
            // Metrics only; building an overlay here would rasterise the glyphs
            // just to learn the line height, then throw them away.
            let th = overlay::line_height(px);
            overlays.push(line_at(text, window_h as i32 - margin - th as i32));
        }

        let Some(wl) = self.host.screen.wl.as_mut() else {
            return false;
        };
        let p = &mut wl.presentation;
        if p.presenter.present(
            &scaled,
            &p.surface,
            self.session.view.offset_x,
            self.session.view.offset_y,
            &overlays,
        ) {
            // Whole buffer: margins are filled with background too.
            // Must be `damage_buffer`, not `damage`: under fractional scale
            // surface coords can land outside the buffer and never recomposite.
            let (bw, bh) = p.presenter.size();
            p.surface.damage_buffer(0, 0, bw, bh);
            // Before the commit: winit drives its frame callbacks from this,
            // so after the commit it is already a frame too late.
            wl.window.pre_present_notify();
            p.surface.commit();
            // winit only flushes after its own requests; flush or this stalls.
            flush(&p.conn);
            self.host.screen.dirty = false;
            // Reached the compositor.
            false
        } else {
            // All buffers held: the frame is still owed, so arm a retry
            // instead of losing it.
            self.host.screen.dirty = true;
            // Retried: the caller re-arms the redraw, the timer only paces it.
            true
        }
    }
}
