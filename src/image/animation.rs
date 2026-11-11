//! Animated images: every frame decoded, linearised one at a time on display.

use std::path::PathBuf;
use std::time::Duration;

use image::RgbaImage;
use rayon::prelude::*;

use super::buffer::{CHANNELS, WorkingImage};
use super::cms::ToLinear;
use super::transform::Turn;

/// What the container header said about an animation as a whole.
pub struct Header {
    /// Cycles the file asks for; `None` repeats forever.
    pub cycles: Option<u32>,
    /// How the frames reach linear light.
    pub to_linear: ToLinear,
    /// Decoded bit depth, for the info overlay.
    pub bit_depth: u8,
    /// Carried into every frame so it knows where it came from.
    pub path: PathBuf,
}

/// Every frame of one animation. Each frame is already a full canvas, so the
/// only thing playback has to do is swap which one is on screen.
pub struct Animation {
    frames: Vec<RgbaImage>,
    /// How long each frame stays up; `frames[i]` for `delays[i]`.
    delays: Vec<Duration>,
    cycles: Option<u32>,
    /// Built once: every frame goes through the same conversion.
    to_linear: ToLinear,
    path: PathBuf,
    bit_depth: u8,
}

impl Animation {
    /// The loader enforces the frame and area budgets as it reads, so they are
    /// not re-checked here.
    /// `frames` must not be empty; the loader checks that.
    pub fn new(frames: Vec<(RgbaImage, Duration)>, header: Header) -> Self {
        let (buffers, delays): (Vec<_>, Vec<_>) = frames.into_iter().unzip();

        tracing::debug!(
            "animation {}: {} frames",
            header.path.display(),
            buffers.len()
        );
        Self {
            frames: buffers,
            delays,
            cycles: header.cycles,
            to_linear: header.to_linear,
            path: header.path,
            bit_depth: header.bit_depth,
        }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// How long frame `index` stays up.
    pub fn delay(&self, index: usize) -> Duration {
        self.delays[index]
    }

    pub fn cycles(&self) -> Option<u32> {
        self.cycles
    }

    /// Rotates or mirrors every frame, so the effect survives playback.
    pub fn apply(&mut self, turn: Turn) {
        for frame in &mut self.frames {
            let (w, h) = (frame.width(), frame.height());
            let mut raw = std::mem::take(frame).into_raw();
            turn.apply(w as usize, h as usize, &mut raw, CHANNELS);
            let (w, h) = if turn.swaps_extents() { (h, w) } else { (w, h) };
            *frame = RgbaImage::from_raw(w, h, raw).expect("a transform keeps the pixel count");
        }
    }

    /// Converts one frame to linear light; everything else is per file.
    pub fn frame(&self, index: usize) -> WorkingImage {
        let buf = &self.frames[index];
        let (w, h) = (buf.width(), buf.height());
        let mut data: Vec<f32> = buf
            .as_raw()
            .par_iter()
            .map(|&s| f32::from(s) / 255.0)
            .collect();
        self.to_linear.apply(&mut data);

        // Frames were checked non-empty at load; the fallback only keeps a
        // broken invariant from becoming a panic on the render path.
        WorkingImage::from_rgba(w, h, data, self.path.clone(), Some(self.bit_depth))
            .unwrap_or_else(|| WorkingImage::new(1, 1))
    }
}
