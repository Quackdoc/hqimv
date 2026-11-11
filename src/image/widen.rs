//! Widening decoded samples into the f32 RGBA working buffer.

use image::DynamicImage;
use rayon::prelude::*;

use super::buffer::CHANNELS;

/// Sample representation of the decoded image.
#[derive(Clone, Copy)]
enum Kind {
    U8,
    U16,
    F32,
}

/// Byte offsets within a source pixel for each destination channel, and the
/// divisor that normalises a sample to `0..=1`.
struct Plan {
    at: [usize; CHANNELS],
    div: f32,
}

#[inline(always)]
fn sample<const KIND: u8>(src: &[u8], at: usize) -> f32 {
    match KIND {
        0 => f32::from(src[at]),
        1 => f32::from(u16::from_ne_bytes([src[at], src[at + 1]])),
        _ => f32::from_ne_bytes([src[at], src[at + 1], src[at + 2], src[at + 3]]),
    }
}

/// One monomorphisation per sample type and alpha shape, so the per-pixel loop
/// is four widening reads and four writes with no branches or bounds checks.
fn fill<const KIND: u8, const ALPHA: bool>(
    bytes: &[u8],
    stride: usize,
    out: &mut [f32],
    plan: &Plan,
) {
    out.par_chunks_mut(CHANNELS)
        .zip(bytes.par_chunks(stride))
        .for_each(|(dst, src)| {
            dst[0] = sample::<KIND>(src, plan.at[0]) / plan.div;
            dst[1] = sample::<KIND>(src, plan.at[1]) / plan.div;
            dst[2] = sample::<KIND>(src, plan.at[2]) / plan.div;
            if ALPHA {
                dst[3] = sample::<KIND>(src, plan.at[3]) / plan.div;
            } else {
                dst[3] = 1.0;
            }
        });
}

/// Widens `img` into a fresh f32 RGBA buffer straight from the decoder's bytes,
/// avoiding the extra image-sized buffer `to_rgb32f` would allocate. Luma is
/// copied into all three channels, which keeps a grey pixel grey.
pub fn to_rgba_f32(img: &DynamicImage) -> Vec<f32> {
    let bytes = img.as_bytes();
    let mut out = vec![0f32; img.width() as usize * img.height() as usize * CHANNELS];

    // Samples per pixel, sample type, whether the source is grey, and whether
    // it carries alpha. Every variant is listed, so a new one from `image`
    // needs handling here rather than falling through and silently producing
    // a black image.
    let (channels, kind, grey, alpha) = match img {
        DynamicImage::ImageLuma8(_) => (1, Kind::U8, true, false),
        DynamicImage::ImageLumaA8(_) => (2, Kind::U8, true, true),
        DynamicImage::ImageLuma16(_) => (1, Kind::U16, true, false),
        DynamicImage::ImageLumaA16(_) => (2, Kind::U16, true, true),
        DynamicImage::ImageRgb8(_) => (3, Kind::U8, false, false),
        DynamicImage::ImageRgba8(_) => (4, Kind::U8, false, true),
        DynamicImage::ImageRgb16(_) => (3, Kind::U16, false, false),
        DynamicImage::ImageRgba16(_) => (4, Kind::U16, false, true),
        DynamicImage::ImageRgb32F(_) => (3, Kind::F32, false, false),
        DynamicImage::ImageRgba32F(_) => (4, Kind::F32, false, true),
        // A layout added to `image` later: let it do the conversion.
        other => return other.to_rgba32f().into_raw(),
    };

    let (step, div) = match kind {
        Kind::U8 => (1usize, 255.0),
        Kind::U16 => (2, 65535.0),
        Kind::F32 => (4, 1.0),
    };
    let stride = channels * step;
    let last = stride - step;
    let plan = Plan {
        at: if grey {
            [0, 0, 0, last]
        } else {
            [0, step, 2 * step, last]
        },
        div,
    };

    match (kind, alpha) {
        (Kind::U8, false) => fill::<0, false>(bytes, stride, &mut out, &plan),
        (Kind::U8, true) => fill::<0, true>(bytes, stride, &mut out, &plan),
        (Kind::U16, false) => fill::<1, false>(bytes, stride, &mut out, &plan),
        (Kind::U16, true) => fill::<1, true>(bytes, stride, &mut out, &plan),
        (Kind::F32, false) => fill::<2, false>(bytes, stride, &mut out, &plan),
        (Kind::F32, true) => fill::<2, true>(bytes, stride, &mut out, &plan),
    }

    out
}
