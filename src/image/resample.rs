//! Separable CPU resampling (Lanczos3, Mitchell, bilinear).
//! Kernels stretch by `1/scale` when minifying to avoid aliasing.

use std::fmt;

use rayon::prelude::*;

use super::buffer::{CHANNELS, WorkingImage};
use crate::names::Named;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    /// Windowed sinc with radius 3. Sharpest, slight ringing on hard edges.
    #[default]
    Lanczos3,
    /// B=1/3, C=1/3 cubic. Softer than Lanczos, no ringing.
    Mitchell,
    /// Bilinear. Fast, blurry when minifying.
    Bilinear,
}

impl Named for Filter {
    const NAMES: &'static [(Self, &'static [&'static str])] = &[
        (Self::Lanczos3, &["lanczos3", "lanczos"]),
        (Self::Mitchell, &["mitchell", "cubic"]),
        (Self::Bilinear, &["bilinear", "linear"]),
    ];
}

impl Filter {
    fn radius(self) -> f64 {
        match self {
            Self::Lanczos3 => 3.0,
            Self::Mitchell => 2.0,
            Self::Bilinear => 1.0,
        }
    }

    // May return negatives (Lanczos); the caller normalizes to sum 1.
    fn kernel(self, x: f64) -> f64 {
        match self {
            Self::Lanczos3 => {
                if x == 0.0 {
                    return 1.0;
                }
                if x.abs() >= 3.0 {
                    return 0.0;
                }
                // Product form tends to 1 as x -> 0 without cancellation.
                let px = std::f64::consts::PI * x;
                let sinc = px.sin() / px;
                let window = (px / 3.0).sin() / (px / 3.0);
                sinc * window
            }
            Self::Mitchell => {
                // Mitchell-Netravali with B = C = 1/3, constants folded.
                let x = x.abs();
                let x2 = x * x;
                if x < 1.0 {
                    (7.0 * x2 * x - 12.0 * x2 + 16.0 / 3.0) / 6.0
                } else if x < 2.0 {
                    (-7.0 / 3.0 * x2 * x + 12.0 * x2 - 20.0 * x + 32.0 / 3.0) / 6.0
                } else {
                    0.0
                }
            }
            Self::Bilinear => {
                if x.abs() < 1.0 {
                    1.0 - x.abs()
                } else {
                    0.0
                }
            }
        }
    }
}

/// Contiguous taps from `starts[out]`; out-of-image taps weigh 0.
#[derive(Debug, Clone)]
struct Weights {
    starts: Vec<usize>,
    weights: Vec<f32>,
    taps: usize,
}

impl Weights {
    fn row(&self, out: usize) -> &[f32] {
        &self.weights[out * self.taps..(out + 1) * self.taps]
    }
}

fn build_weights(src_len: u32, dst_len: u32, filter: Filter) -> Weights {
    let src_len = src_len.max(1) as f64;
    let dst_len = dst_len.max(1) as f64;

    let scale = (src_len / dst_len).max(1.0);
    let radius = filter.radius() * scale;
    let taps = ((2.0 * radius).ceil() as usize).max(1);

    let mut starts = Vec::with_capacity(dst_len as usize);
    let mut weights = Vec::with_capacity(dst_len as usize * taps);
    let mut row: Vec<f64> = Vec::with_capacity(taps);

    let last_src = src_len as i64 - 1;
    for out in 0..dst_len as usize {
        let centre = (out as f64 + 0.5) * (src_len / dst_len) - 0.5;
        let first = (centre - radius).ceil() as i64;
        // The kernel must be evaluated from the clamped `start` the taps are
        // consumed from, not the unclamped `first`.
        let start = first.clamp(0, last_src);
        starts.push(start as usize);

        let mut sum = 0.0f64;
        row.clear();
        for i in 0..taps {
            let src = start + i as i64;
            // Zero past the end instead of clamping, which would brighten edges.
            let w = if src > last_src {
                0.0
            } else {
                filter.kernel((src as f64 - centre) / scale)
            };
            sum += w;
            row.push(w);
        }
        // Normalize so flat areas stay flat.
        let inv = if sum.abs() > f64::EPSILON {
            1.0 / sum
        } else {
            0.0
        };
        weights.extend(row.iter().map(|&w| (w * inv) as f32));
    }

    Weights {
        starts,
        weights,
        taps,
    }
}

/// Matching geometry returns a clone rather than resampling.
pub fn resize(src: &WorkingImage, dst_w: u32, dst_h: u32, filter: Filter) -> WorkingImage {
    if dst_w == 0 || dst_h == 0 {
        return WorkingImage::new(dst_w.max(1), dst_h.max(1));
    }
    if src.width() == dst_w && src.height() == dst_h {
        return src.clone();
    }

    let n = CHANNELS;
    let (src_w, src_h) = src.size();
    let (src_row, mid_row) = (src_w as usize * n, dst_w as usize * n);

    // Horizontal pass into `mid`, then vertical pass into `out`.
    let hx = build_weights(src_w, dst_w, filter);
    let mut mid = vec![0f32; mid_row * src_h as usize];
    pass(src.data(), &mut mid, mid_row, n, &hx, |y, x| {
        (x, y * src_row + hx.starts[x] * n)
    });

    let vy = build_weights(src_h, dst_h, filter);
    let mut out = vec![0f32; mid_row * dst_h as usize];
    pass(&mid, &mut out, mid_row, mid_row, &vy, |y, x| {
        (y, vy.starts[y] * mid_row + x * n)
    });

    WorkingImage::from_rgba(
        dst_w,
        dst_h,
        out,
        src.source_path().to_path_buf(),
        src.source_bit_depth(),
    )
    .expect("output buffer was sized to match the geometry")
}

/// One separable filter pass. `at(row, x)` gives the weight set and the index
/// of the first source sample for output pixel `x` of output row `row`; later
/// taps follow `tap_stride` apart.
fn pass(
    src: &[f32],
    out: &mut [f32],
    out_row: usize,
    tap_stride: usize,
    weights: &Weights,
    at: impl Fn(usize, usize) -> (usize, usize) + Sync,
) {
    out.par_chunks_mut(out_row)
        .enumerate()
        .for_each(|(y, dst_row)| {
            for (x, dst) in dst_row.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
                let (set, first) = at(y, x);
                let mut acc = [0f32; CHANNELS];
                for (t, &w) in weights.row(set).iter().enumerate() {
                    if w == 0.0 {
                        continue;
                    }
                    let s = first + t * tap_stride;
                    for c in 0..CHANNELS {
                        acc[c] += w * src[s + c];
                    }
                }
                *dst = acc;
            }
        });
}

/// Non-finite/non-positive factors become 1.0.
pub fn sane_factor(f: f64) -> f64 {
    if f.is_finite() && f > 0.0 { f } else { 1.0 }
}

/// Scales and rounds to at least one pixel.
pub fn scaled_len(len: u32, factor: f64) -> u32 {
    ((len as f64 * factor).round() as u32).max(1)
}

fn fit(w: u32, h: u32, max_w: u32, max_h: u32, cover: bool) -> (u32, u32) {
    if w == 0 || h == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    let sw = max_w as f64 / w as f64;
    let sh = max_h as f64 / h as f64;
    let scale = if cover { sw.max(sh) } else { sw.min(sh) };
    (scaled_len(w, scale), scaled_len(h, scale))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScaleMode {
    /// Largest size that fits inside the window, preserving aspect ratio.
    #[default]
    Fit,
    /// One source pixel per device pixel, centred.
    OneToOne,
    /// Fill the window completely, cropping the overflow.
    Fill,
}

impl Named for ScaleMode {
    const NAMES: &'static [(Self, &'static [&'static str])] = &[
        (Self::Fit, &["fit"]),
        (Self::OneToOne, &["1:1", "one", "one-to-one", "actual"]),
        (Self::Fill, &["fill"]),
    ];
}

impl fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl ScaleMode {
    pub fn next(self) -> Self {
        match self {
            Self::Fit => Self::OneToOne,
            Self::OneToOne => Self::Fill,
            Self::Fill => Self::Fit,
        }
    }
}

/// OneToOne applies `scale_factor` for HiDPI.
pub fn output_size(
    mode: ScaleMode,
    src: (u32, u32),
    window: (u32, u32),
    scale_factor: f64,
) -> (u32, u32) {
    match mode {
        ScaleMode::Fit => fit(src.0, src.1, window.0, window.1, false),
        ScaleMode::Fill => fit(src.0, src.1, window.0, window.1, true),
        ScaleMode::OneToOne => {
            let f = sane_factor(scale_factor);
            (scaled_len(src.0, f), scaled_len(src.1, f))
        }
    }
}
