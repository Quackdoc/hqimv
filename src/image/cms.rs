//! CMS: color management via moxcms.

use std::sync::Arc;

use moxcms::{
    CicpColorPrimaries, CicpProfile, ColorProfile, Layout, MatrixCoefficients,
    TransferCharacteristics, TransformF32Executor, TransformOptions,
};
use rayon::prelude::*;

use super::buffer::CHANNELS;
use image::metadata::{Cicp as ImageCicp, CicpTransferCharacteristics, CicpVideoFullRangeFlag};

/// Authoritative source-to-linear description for one image.
#[derive(Debug, Clone)]
pub enum ColorMetadata {
    Linear,
    Cicp(CicpProfile),
    Icc(Vec<u8>),
    /// No color info; assume sRGB.
    None,
}

impl ColorMetadata {
    /// Picks the authoritative source: ICC outranks CICP, linear short-circuits.
    pub fn detect(icc: Option<Vec<u8>>, cicp: ImageCicp) -> Self {
        // dont replace this with a match
        if cicp.transfer == CicpTransferCharacteristics::Linear {
            Self::Linear
        } else if let Some(bytes) = icc {
            Self::Icc(bytes)
        } else if let Some(c) = cicp_source(cicp) {
            Self::Cicp(c)
        } else {
            Self::None
        }
    }
}
fn cicp_source(c: ImageCicp) -> Option<CicpProfile> {
    let transfer = TransferCharacteristics::try_from(c.transfer as u8).ok()?;
    if transfer == TransferCharacteristics::Unspecified {
        return None;
    }
    Some(CicpProfile {
        color_primaries: CicpColorPrimaries::try_from(c.primaries as u8)
            .unwrap_or(CicpColorPrimaries::Unspecified),
        transfer_characteristics: transfer,
        matrix_coefficients: MatrixCoefficients::try_from(c.matrix as u8)
            .unwrap_or(MatrixCoefficients::Unspecified),
        full_range: c.full_range == CicpVideoFullRangeFlag::FullRange,
    })
}

fn linear_dst() -> ColorProfile {
    ColorProfile::new_from_cicp(CicpProfile {
        color_primaries: CicpColorPrimaries::Bt709,
        transfer_characteristics: TransferCharacteristics::Linear,
        matrix_coefficients: MatrixCoefficients::Identity,
        full_range: true,
    })
}

/// How a colour description turned out once prepared.
enum Prepared {
    /// Already linear; nothing to do.
    Identity,
    Transform {
        tag: &'static str,
        executor: Arc<TransformF32Executor>,
    },
    /// Nothing usable; gamma 2.2 every time.
    Gamma,
}

/// A prepared source-to-linear transform. Building it costs milliseconds, so
/// animation prepares it once instead of per frame.
pub struct ToLinear {
    prepared: Prepared,
    /// Ceiling on the transform scratch, in bytes. The executor is out-of-place,
    /// so it needs a second buffer, but that buffer only has to be as large as
    /// one band per worker rather than the whole image.
    scratch_budget: usize,
}

impl ToLinear {
    pub fn new(metadata: &ColorMetadata, scratch_budget: usize) -> Self {
        let prepared = match metadata {
            ColorMetadata::Linear => Prepared::Identity,
            ColorMetadata::Icc(bytes) => match ColorProfile::new_from_slice(bytes).ok() {
                Some(profile) => build(&profile, "icc"),
                None => {
                    tracing::warn!("icc parse failed, gamma fallback");
                    Prepared::Gamma
                }
            },
            ColorMetadata::Cicp(c) => build(&ColorProfile::new_from_cicp(*c), "cicp"),
            ColorMetadata::None => build(&ColorProfile::new_srgb(), "moxcms"),
        };
        Self {
            prepared,
            scratch_budget,
        }
    }

    pub fn apply(&self, data: &mut [f32]) {
        match &self.prepared {
            Prepared::Identity => {}
            Prepared::Transform { tag, executor } => run(executor, data, tag, self.scratch_budget),
            Prepared::Gamma => gamma_fallback(data),
        }
    }
}

// Out-of-place: one path covers matrix-shaper and LUT-based profiles.
fn build(src: &ColorProfile, tag: &'static str) -> Prepared {
    match src.create_transform_f32(
        Layout::Rgba,
        &linear_dst(),
        Layout::Rgba,
        TransformOptions::default(),
    ) {
        Ok(executor) => Prepared::Transform { tag, executor },
        Err(err) => {
            tracing::warn!("{tag} transform build failed ({err}), gamma fallback");
            Prepared::Gamma
        }
    }
}

fn run(executor: &Arc<TransformF32Executor>, data: &mut [f32], tag: &str, scratch_budget: usize) {
    let threads = rayon::current_num_threads().max(1);

    // RGBA-aligned so each band is a valid strip, and never wider than the work
    // actually needs. Alignment comes last: a lane has to be a whole number of
    // pixels, and the clamp above does not preserve that.
    let mut band_len = (scratch_budget / (threads * size_of::<f32>())).max(CHANNELS);
    band_len = band_len.min(data.len().div_ceil(threads).max(CHANNELS));
    band_len -= band_len % CHANNELS;

    // One scratch per worker, reused across rounds so each worker stays on its
    // own buffer.
    let mut scratch: Vec<Vec<f32>> = (0..threads).map(|_| vec![0f32; band_len]).collect();
    for start in (0..data.len()).step_by(band_len * threads) {
        data[start..]
            .par_chunks_mut(band_len)
            .take(threads)
            .zip(scratch.par_iter_mut())
            .for_each(|(band, buf)| {
                let dst = &mut buf[..band.len()];
                if let Err(err) = executor.transform(band, dst) {
                    tracing::warn!("{tag} band transform failed: {err}");
                } else {
                    band.copy_from_slice(dst);
                }
            });
    }
}

fn gamma_fallback(data: &mut [f32]) {
    // Alpha stays put: only the colour channels are gamma-encoded.
    data.par_chunks_mut(CHANNELS).for_each(|px| {
        px[0] = transfer(px[0], GAMMA);
        px[1] = transfer(px[1], GAMMA);
        px[2] = transfer(px[2], GAMMA);
    });
}

/// Display gamma as a plain power, not the sRGB piecewise curve. Lives here
/// because the fallback, the forward curve, and its inverse must agree on it.
const GAMMA: f32 = 2.2;

/// `v^gamma`; values above 1.0 pass through for scene-referred float data.
fn transfer(v: f32, gamma: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    if v >= 1.0 {
        return v;
    }
    v.powf(gamma)
}

/// Encoded value to linear light, for the display fallback.
pub fn encoded_to_linear(v: f32) -> f32 {
    transfer(v, GAMMA)
}

/// Linear light back to an encoded value.
pub fn linear_to_encoded(v: f32) -> f32 {
    transfer(v, 1.0 / GAMMA)
}
