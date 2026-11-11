//! The `f32` RGBA working buffer: pixels, size, source, and metadata.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::transform::Turn;

pub const CHANNELS: usize = 4;

pub mod channel {
    pub const R: usize = 0;
    pub const G: usize = 1;
    pub const B: usize = 2;
    pub const A: usize = 3;
}

#[derive(Debug, Clone)]
pub struct WorkingImage {
    width: u32,
    height: u32,
    data: Vec<f32>,
    source_path: PathBuf,
    /// Decoded bit depth, for the info overlay and status line.
    source_bit_depth: Option<u8>,
    /// Cached from `data` at construction. Rotating and quantizing cannot make
    /// an opaque image translucent, so this stays true for the image's life,
    /// and the draw path asks every frame.
    opaque: bool,
}

impl WorkingImage {
    /// Wraps a decoded buffer; `source_bit_depth` feeds the info overlay.
    /// Returns `None` on zero size or length mismatch.
    pub fn from_rgba(
        width: u32,
        height: u32,
        data: Vec<f32>,
        source_path: impl Into<PathBuf>,
        source_bit_depth: Option<u8>,
    ) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if data.len() != width as usize * height as usize * CHANNELS {
            return None;
        }
        let opaque = opaque(&data);
        Some(Self {
            width,
            height,
            data,
            source_path: source_path.into(),
            source_bit_depth,
            opaque,
        })
    }

    /// One fully transparent pixel.
    pub fn blank() -> Self {
        Self {
            width: 1,
            height: 1,
            data: vec![0.0; CHANNELS],
            source_path: PathBuf::new(),
            source_bit_depth: None,
            opaque: false,
        }
    }

    pub fn new(width: u32, height: u32) -> Self {
        let count = width as usize * height as usize * CHANNELS;
        let mut data = vec![0.0; count];
        for px in data.as_chunks_mut::<CHANNELS>().0 {
            px[channel::A] = 1.0;
        }
        Self {
            width,
            height,
            data,
            source_path: PathBuf::new(),
            source_bit_depth: None,
            opaque: true,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn source_bit_depth(&self) -> Option<u8> {
        self.source_bit_depth
    }

    pub fn is_opaque(&self) -> bool {
        self.opaque
    }

    pub fn apply(&mut self, turn: Turn) {
        let (w, h) = (self.width as usize, self.height as usize);
        turn.apply(w, h, &mut self.data, CHANNELS);
        if turn.swaps_extents() {
            std::mem::swap(&mut self.width, &mut self.height);
        }
    }

    /// Composites over `background`, keeping whatever alpha remains. An opaque
    /// background removes alpha; a transparent one leaves the image unchanged.
    pub fn composite_over(mut self, background: [f32; 4]) -> Self {
        let bg_a = background[3].clamp(0.0, 1.0);
        if self.is_opaque() || bg_a == 0.0 {
            return self;
        }
        self.data.par_chunks_mut(CHANNELS).for_each(|px| {
            let a = px[channel::A].clamp(0.0, 1.0);
            let out_a = a + bg_a * (1.0 - a);
            for c in 0..3 {
                px[c] = (px[c] * a + background[c] * bg_a * (1.0 - a)) / out_a;
            }
            px[channel::A] = out_a;
        });
        self.opaque = bg_a >= 1.0;
        self
    }
}

/// True when no pixel is translucent.
fn opaque(data: &[f32]) -> bool {
    data.as_chunks::<CHANNELS>()
        .0
        .iter()
        .all(|px| px[channel::A] >= 1.0)
}
