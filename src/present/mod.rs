//! Presentation: turning a linear image into compositor pixels.
//!
//! [`format`](mod@format) holds the format table and the packer. This module owns the
//! `wl_shm` memory pool and the buffers drawn into it.

pub mod format;

use std::ffi::c_void;
use std::ptr::NonNull;

use smithay_client_toolkit::shm::slot::{Buffer, CreateBufferError, SlotPool};
use smithay_client_toolkit::shm::{CreatePoolError, Shm};
use wayland_client::Proxy;
use wayland_client::protocol::wl_surface;

use crate::image::WorkingImage;
use crate::image::overlay::TextOverlay;

pub use format::{ShmFormat, negotiate};

const BUFFER_COUNT: usize = 3;

pub struct Presenter {
    pool: SlotPool,
    format: ShmFormat,
    width: i32,
    height: i32,
    buffers: Vec<Buffer>,
    background: [f32; 4],
}

#[derive(Debug, thiserror::Error)]
pub enum PresentError {
    #[error("cannot create the shared-memory pool: {0}")]
    Pool(#[from] CreatePoolError),
    #[error("cannot create a shared-memory buffer: {0}")]
    Buffer(#[from] CreateBufferError),
}

impl Presenter {
    /// Requires a completed shm roundtrip so formats() is populated.
    pub fn new(
        shm: &Shm,
        format: ShmFormat,
        width: u32,
        height: u32,
        background: [f32; 4],
    ) -> Result<Self, PresentError> {
        let (pool, buffers, width, height) = Self::build_pool(shm, &format, width, height)?;
        Ok(Self {
            pool,
            format,
            width,
            height,
            buffers,
            background,
        })
    }

    // wl_buffer.create rejects zero-sized buffers.
    fn build_pool(
        shm: &Shm,
        format: &ShmFormat,
        width: u32,
        height: u32,
    ) -> Result<(SlotPool, Vec<Buffer>, i32, i32), PresentError> {
        let width = width.max(1) as i32;
        let height = height.max(1) as i32;
        let stride = format.stride(width as u32);
        let frame_bytes = stride * height as usize;

        // Size for all buffers up front; growing mid-frame races the compositor.
        let mut pool = SlotPool::new(frame_bytes * BUFFER_COUNT, shm)?;

        // `create_buffer` takes its own slot, so the pool is sized for the
        // buffers alone.
        let mut buffers = Vec::with_capacity(BUFFER_COUNT);
        for _ in 0..BUFFER_COUNT {
            let (buffer, _canvas) =
                pool.create_buffer(width, height, stride as i32, format.code)?;
            buffers.push(buffer);
        }
        tracing::debug!("buffer pool: {width}x{height} ({frame_bytes} bytes per frame)");
        Ok((pool, buffers, width, height))
    }

    fn allocate(&mut self, shm: &Shm, width: u32, height: u32) -> Result<(), PresentError> {
        // Dropped before the pool is replaced; it can't free with live mappings.
        // Built first so a partial failure leaves no buffers (read as a retry)
        // rather than a half-replaced set.
        self.buffers.clear();
        let (pool, buffers, width, height) = Self::build_pool(shm, &self.format, width, height)?;
        self.pool = pool;
        self.buffers = buffers;
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Surface size in pixels.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// wl_buffer geometry is immutable, so resize makes new buffers.
    pub fn resize(&mut self, shm: &Shm, width: u32, height: u32) -> Result<(), PresentError> {
        if width as i32 == self.width && height as i32 == self.height {
            return Ok(());
        }
        self.allocate(shm, width, height)
    }

    /// Returns false when all buffers are held; caller retries next frame.
    pub fn present(
        &mut self,
        image: &WorkingImage,
        surface: &wl_surface::WlSurface,
        ox: i32,
        oy: i32,
        overlays: &[TextOverlay],
    ) -> bool {
        let stride = self.format.stride(self.width as u32);
        let needed = stride * self.height as usize;

        let free = self
            .buffers
            .iter()
            .position(|b| !b.slot().has_active_buffers());
        let Some(index) = free else {
            tracing::debug!("all buffers are held by the compositor, skipping frame");
            return false;
        };

        let Some(canvas) = self.pool.canvas(&self.buffers[index]) else {
            return false;
        };
        let canvas = &mut canvas[..needed];

        // Margins are real pixels; clear leftover pool memory.
        let [r, g, b, a] = self.background;
        format::fill(canvas, &self.format, [r, g, b], a);

        let (dw, dh) = (image.width() as usize, image.height() as usize);
        let (ox, oy) = (
            ((self.width - dw as i32) / 2) + ox,
            ((self.height - dh as i32) / 2) + oy,
        );
        format::encode_blit(image, &self.format, stride, canvas, ox, oy);

        // Window-space overlays; drawn after the blit.
        for overlay_text in overlays {
            format::blit_text_overlay(
                canvas,
                &self.format,
                stride,
                self.width as u32,
                self.height as u32,
                overlay_text,
            );
        }

        // Skip on stale release view rather than double-attach.
        match self.buffers[index].attach_to(surface) {
            Ok(()) => true,
            Err(err) => {
                tracing::debug!("could not attach buffer: {err:?}");
                false
            }
        }
    }
}

/// Wraps winit's live wl_surface proxy.
pub fn wrap_surface(
    conn: &wayland_client::Connection,
    ptr: NonNull<c_void>,
) -> Option<wl_surface::WlSurface> {
    // SAFETY: ptr is a live wl_surface proxy from raw_window_handle;
    // from_ptr only reads interface/id, never takes ownership.
    let id = unsafe {
        wayland_backend::sys::client::ObjectId::from_ptr(
            wl_surface::WlSurface::interface(),
            ptr.as_ptr().cast(),
        )
    }
    .ok()?;
    wl_surface::WlSurface::from_id(conn, id).ok()
}
