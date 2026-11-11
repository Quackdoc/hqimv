//! Choosing a `wl_shm` format and packing linear floats into it.
//!
//! `wl_shm` names read backwards compared to memory: `xrgb8888` is documented
//! `[31:0] x:R:G:B`, so the lowest byte is blue. `rgb_order` stores the channel
//! per memory slot directly, copied from the protocol definition, so packing
//! never infers byte order. The high slot is padding, or alpha when present.

use crate::image::buffer::{CHANNELS, WorkingImage, channel};
use crate::image::cms::{encoded_to_linear, linear_to_encoded};
use crate::image::overlay::TextOverlay;
use rayon::prelude::*;

use wayland_client::protocol::wl_shm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmFormat {
    pub code: wl_shm::Format,
    pub bpp: usize,
    pub depth: u8,
    pub is_float: bool,
    pub has_alpha: bool,
    /// Channel per memory slot as 0=R,1=G,2=B; reversed from protocol order.
    pub rgb_order: [usize; 3],
    pub name: &'static str,
}

impl ShmFormat {
    /// Row stride in bytes.
    pub const fn stride(&self, width: u32) -> usize {
        (width as usize) * self.bpp
    }
}

const fn fmt(
    code: wl_shm::Format,
    bpp: usize,
    depth: u8,
    is_float: bool,
    has_alpha: bool,
    rgb_order: [usize; 3],
    name: &'static str,
) -> ShmFormat {
    ShmFormat {
        code,
        bpp,
        depth,
        is_float,
        has_alpha,
        rgb_order,
        name,
    }
}

// Layouts are the protocol's `[n:0]` fields, high to low. Memory slot order is
// reversed from that; 8-bit `xrgb`/`argb` names read backwards (red in the high
// byte), so their slots are B, G, R.
const BGR: [usize; 3] = [2, 1, 0];
const RGB: [usize; 3] = [0, 1, 2];

#[rustfmt::skip]
const FORMATS: &[ShmFormat] = &[
    fmt(wl_shm::Format::Xbgr16161616f, 8, 16, true, false, RGB, "xbgr16161616f"),
    fmt(wl_shm::Format::Abgr16161616f, 8, 16, true, true, RGB, "abgr16161616f"),
    fmt(wl_shm::Format::Xbgr16161616, 8, 16, false, false, RGB, "xbgr16161616"),
    fmt(wl_shm::Format::Abgr16161616, 8, 16, false, true, RGB, "abgr16161616"),
    fmt(wl_shm::Format::Xbgr2101010, 4, 10, false, false, RGB, "xbgr2101010"),
    fmt(wl_shm::Format::Abgr2101010, 4, 10, false, true, RGB, "abgr2101010"),
    fmt(wl_shm::Format::Xrgb8888, 4, 8, false, false, BGR, "xrgb8888"),
    fmt(wl_shm::Format::Argb8888, 4, 8, false, true, BGR, "argb8888"),
    fmt(wl_shm::Format::Xbgr8888, 4, 8, false, false, RGB, "xbgr8888"),
    fmt(wl_shm::Format::Abgr8888, 4, 8, false, true, RGB, "abgr8888"),
];

/// Best advertised format; `wants_alpha` only reorders the preference.
///
/// Without an alpha slot `pack` writes an opaque padding word, so a compositor
/// offering only opaque formats is still usable with a translucent image or
/// background (it just shows black), and must not be rejected here.
pub fn negotiate(advertised: &[wl_shm::Format], wants_alpha: bool) -> Option<ShmFormat> {
    FORMATS
        .iter()
        .find(|f| f.has_alpha == wants_alpha && advertised.contains(&f.code))
        .or_else(|| FORMATS.iter().find(|f| advertised.contains(&f.code)))
        .copied()
}

/// Case-insensitive; ignores -/_ separators.
pub fn by_name(name: &str) -> Option<ShmFormat> {
    fn canonical(name: &str) -> String {
        name.trim().to_ascii_lowercase().replace(['-', '_'], "")
    }

    let want = canonical(name);
    FORMATS.iter().find(|f| canonical(f.name) == want).copied()
}

pub fn fill(buf: &mut [u8], format: &ShmFormat, linear: [f32; 3], alpha: f32) {
    let px = buf.len() / format.bpp;
    if px == 0 {
        return;
    }
    // The word is the same for every pixel; `pack` is three `powf` calls.
    let word = pack(&linear, format, alpha);
    for x in 0..px {
        write_word(&mut buf[x * format.bpp..], word, format);
    }
}

/// Blits into a window buffer at (ox, oy); clips to bounds.
pub fn encode_blit(
    image: &WorkingImage,
    format: &ShmFormat,
    stride: usize,
    buf: &mut [u8],
    ox: i32,
    oy: i32,
) {
    let (sw, sh) = image.size();
    let (dst_w, dst_h) = (sw as usize, sh as usize);
    let src = image.data();
    let row_len = sw as usize * CHANNELS;

    // Rows are disjoint, so this parallelises over them like the crate's other
    // pixel loops.
    let rows = buf.len() / stride;
    let first = oy.max(0) as usize;
    let last = (i64::from(oy) + dst_h as i64).clamp(0, rows as i64) as usize;
    if first >= last {
        return;
    }
    let window_width = (stride / format.bpp) as i64;

    buf[first * stride..last * stride]
        .par_chunks_mut(stride)
        .enumerate()
        .for_each(|(i, row)| {
            let dy = (first + i) as i64;
            let sy = (dy - i64::from(oy)) as usize;
            let right = (i64::from(ox) + dst_w as i64).min(window_width);
            for dx in i64::from(ox).max(0)..right {
                let sx = (dx - i64::from(ox)) as usize;
                let start = sx * CHANNELS;
                let p = &src[sy * row_len + start..sy * row_len + start + CHANNELS];
                let linear = [p[channel::R], p[channel::G], p[channel::B]];
                let word = pack(&linear, format, p[channel::A]);
                let at = dx as usize * format.bpp;
                write_word(&mut row[at..], word, format);
            }
        });
}

fn pack(linear: &[f32; 3], format: &ShmFormat, alpha: f32) -> u64 {
    let chan_bits = format.depth as u32;
    let mut word: u64 = 0;

    // Shift by field width, not 8: 16-bit fields sit at 0, 16, 32.
    for i in 0..3 {
        let v = linear[format.rgb_order[i]];
        let value = if format.is_float {
            quantize_float(v)
        } else {
            quantize(scale_normalized(linear_to_encoded(v), chan_bits), chan_bits)
        };
        word |= (value & mask(chan_bits)) << (chan_bits * i as u32);
    }

    let (high_bits, high_shift) = high_field(format);
    let high = if !format.has_alpha {
        // All-ones padding reads as opaque.
        mask(high_bits)
    } else if format.is_float {
        quantize_float(alpha)
    } else {
        // 2-bit alpha (2:10:10:10) can't blend; 3 = opaque.
        quantize(scale_normalized(alpha, high_bits), high_bits)
    };
    word | ((high & mask(high_bits)) << high_shift)
}

/// Width and shift of the field above the three colour channels.
fn high_field(format: &ShmFormat) -> (u32, u32) {
    match format.depth {
        10 => (2, 30),
        16 => (16, 48),
        _ => (8, 24),
    }
}

fn mask(bits: u32) -> u64 {
    (1u64 << bits) - 1
}

fn max_as_f32(bits: u32) -> f32 {
    mask(bits) as f32
}

// Round, not truncate: truncation darkens resampled edges by half a step.
fn quantize(v: f32, bits: u32) -> u64 {
    (v.clamp(0.0, max_as_f32(bits)) + 0.5) as u64
}

fn scale_normalized(v: f32, bits: u32) -> f32 {
    v.clamp(0.0, 1.0) * max_as_f32(bits)
}

/// The only float `FORMATS` entries are 16-bit half floats.
fn quantize_float(v: f32) -> u64 {
    half::f16::from_f32(v).to_bits() as u64
}

fn write_word(dst: &mut [u8], word: u64, format: &ShmFormat) {
    dst[..format.bpp].copy_from_slice(&word.to_le_bytes()[..format.bpp]);
}

fn read_word(buf: &[u8], off: usize, bpp: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..bpp].copy_from_slice(&buf[off..off + bpp]);
    u64::from_le_bytes(bytes)
}

/// The alpha `pack` wrote; 1.0 for formats without an alpha slot.
fn decode_alpha(word: u64, format: &ShmFormat) -> f32 {
    if !format.has_alpha {
        return 1.0;
    }
    let (bits, shift) = high_field(format);
    let value = (word >> shift) & mask(bits);
    if format.is_float {
        half::f16::from_bits(value as u16).to_f32()
    } else {
        value as f32 / max_as_f32(bits)
    }
}

fn decode_pixel(word: u64, format: &ShmFormat) -> [f32; 3] {
    let bits = format.depth as u32;
    let mut rgb = [0f32; 3];
    for i in 0..3 {
        let value = (word >> (bits * i as u32)) & mask(bits);
        rgb[format.rgb_order[i]] = if format.is_float {
            half::f16::from_bits(value as u16).to_f32()
        } else {
            encoded_to_linear(value as f32 / max_as_f32(bits))
        };
    }
    rgb
}

/// Window-space text; drawn after the blit so pan/zoom never moves it.
pub fn blit_text_overlay(
    buf: &mut [u8],
    format: &ShmFormat,
    stride: usize,
    win_w: u32,
    win_h: u32,
    overlay: &TextOverlay,
) {
    let (tw, th) = overlay.size();
    if tw == 0 || th == 0 {
        return;
    }
    // Pad for the outline halo around the measured text box.
    let pad: i32 = 4;
    let (ox, oy) = overlay.position();
    let rx0 = ox.max(0);
    let ry0 = oy.max(0);
    let rx1 = (i64::from(ox) + i64::from(tw) + i64::from(pad)).min(i64::from(win_w)) as i32;
    let ry1 = (i64::from(oy) + i64::from(th) + i64::from(pad)).min(i64::from(win_h)) as i32;
    if rx1 <= rx0 || ry1 <= ry0 {
        return;
    }
    let (rw, rh) = ((rx1 - rx0) as usize, (ry1 - ry0) as usize);

    let mut region = vec![0f32; rw * rh * CHANNELS];
    for y in 0..rh {
        let row = &buf[(ry0 as usize + y) * stride..];
        for x in 0..rw {
            let off = (rx0 as usize + x) * format.bpp;
            let word = read_word(row, off, format.bpp);
            let rgb = decode_pixel(word, format);
            let p = &mut region[(y * rw + x) * CHANNELS..][..CHANNELS];
            p[0] = rgb[0];
            p[1] = rgb[1];
            p[2] = rgb[2];
            // Keep a translucent backdrop translucent, or the text sits in an
            // opaque box.
            p[3] = decode_alpha(word, format);
        }
    }

    overlay.draw(&mut region, rw as u32, rh as u32, ox - rx0, oy - ry0);

    for y in 0..rh {
        for x in 0..rw {
            let p = &region[(y * rw + x) * CHANNELS..][..CHANNELS];
            let word = pack(&[p[0], p[1], p[2]], format, p[3]);
            let dest = (ry0 as usize + y) * stride + (rx0 as usize + x) * format.bpp;
            write_word(&mut buf[dest..], word, format);
        }
    }
}
