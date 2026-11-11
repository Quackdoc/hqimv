//! Decoding one image from an open reader: stills, GIF and APNG animations,
//! and the decode-budget checks.

use std::io::{BufRead, Seek};
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::metadata::{LoopCount, Orientation};
use image::{
    AnimationDecoder, ColorType, DynamicImage, Frames, ImageDecoder, ImageError, ImageFormat,
    ImageReader, Limits, RgbaImage,
};

use super::{Decoded, LoadError, LoadOptions};
use crate::image::animation::{Animation, Header};
use crate::image::buffer::WorkingImage;
use crate::image::cms::{ColorMetadata, ToLinear};
use crate::image::widen::to_rgba_f32;

/// What `DynamicImage` drops, read before the pixels are consumed: bit depth,
/// EXIF orientation and ICC profile.
fn source_meta(decoder: &mut impl ImageDecoder) -> (u8, Orientation, Option<Vec<u8>>) {
    let color = decoder.color_type();
    (
        (color.bits_per_pixel() / u16::from(color.channel_count())) as u8,
        decoder.orientation().unwrap_or(Orientation::NoTransforms),
        decoder.icc_profile().ok().flatten(),
    )
}

/// Decodes one image from any seekable reader; `id` names it in messages.
pub fn decode<R: BufRead + Seek>(
    reader: ImageReader<R>,
    id: &Path,
    opts: &LoadOptions,
) -> Result<Decoded, LoadError> {
    let mut reader = reader
        .with_guessed_format()
        .map_err(|source| LoadError::Io {
            path: id.display().to_string(),
            source,
        })?;

    // `max_pixels` is the memory policy: the widest thing hqimv will hold is the
    // f32 RGBA working buffer at 16 bytes/pixel, so `max_alloc` is set from it.
    // That bounds the decoder, not the widening pass, which allocates on top.
    let mut limits = Limits::default();
    limits.max_alloc = Some(opts.max_pixels.saturating_mul(16));
    if let Some(edge) = opts.max_dimension {
        limits.max_image_width = Some(edge);
        limits.max_image_height = Some(edge);
    }
    reader.limits(limits.clone());

    match reader.format() {
        Some(ImageFormat::Gif) => load_gif(reader.into_inner(), id, opts, limits),
        Some(ImageFormat::Png) => load_png(reader.into_inner(), id, opts, limits),
        _ => load_still(reader, id, opts),
    }
}

/// Shared tail for the animated formats: drain the frames inside budget and
/// build the animation. The budget is checked per frame because `image` does not
/// charge GIF frames to one shared limit, so the file could otherwise be fully
/// resident before anything objected.
fn animated(
    frames: Frames<'_>,
    path: &Path,
    format: ImageFormat,
    cycles: LoopCount,
    bit_depth: u8,
    icc: Option<Vec<u8>>,
    opts: &LoadOptions,
) -> Result<Decoded, LoadError> {
    let mut out: Vec<(RgbaImage, Duration)> = Vec::new();
    let mut pixels: u64 = 0;

    for frame in frames {
        let frame = frame.map_err(|source| classify(path, source, opts))?;
        let delay = Duration::from(frame.delay()).max(opts.min_frame_delay);
        let buffer = frame.into_buffer();
        if buffer.width() == 0 || buffer.height() == 0 {
            return Err(LoadError::limits(path, "a frame has no pixels"));
        }
        pixels += u64::from(buffer.width()) * u64::from(buffer.height());
        if out.len() >= opts.max_animation_frames || pixels > opts.max_animation_pixels {
            return Err(LoadError::limits(
                path,
                format!(
                    "past the budget of {} frames over {} pixels",
                    opts.max_animation_frames, opts.max_animation_pixels
                ),
            ));
        }
        out.push((buffer, delay));
    }

    if out.is_empty() {
        return Err(LoadError::Decode {
            path: path.display().to_string(),
            source: ImageError::Decoding(image::error::DecodingError::new(
                image::error::ImageFormatHint::Exact(format),
                "the file decoded no frames",
            )),
        });
    }

    Ok(Decoded::Animated(Animation::new(
        out,
        Header {
            cycles: match cycles {
                LoopCount::Infinite => None,
                LoopCount::Finite(n) => Some(n.get()),
            },
            // Neither container describes its colours except through an
            // embedded profile, so without one the still path's assumption
            // applies. CICP and gAMA are not reachable through these decoders.
            to_linear: ToLinear::new(
                &match icc {
                    Some(bytes) => ColorMetadata::Icc(bytes),
                    None => ColorMetadata::None,
                },
                opts.cms_scratch,
            ),
            bit_depth,
            path: path.to_path_buf(),
        },
    )))
}

/// GIFs always have frames; the header decides whether they are ever played.
fn load_gif<R: BufRead + Seek>(
    reader: R,
    path: &Path,
    opts: &LoadOptions,
    limits: Limits,
) -> Result<Decoded, LoadError> {
    let mut decoder = GifDecoder::new(reader).map_err(|source| classify(path, source, opts))?;
    decoder
        .set_limits(limits)
        .map_err(|source| classify(path, source, opts))?;

    // Every frame is a full canvas, so one canvas must fit the budget.
    check_area(
        path,
        decoder.dimensions(),
        opts.max_animation_pixels,
        "limits.max_animation_pixels",
    )?;
    let cycles = decoder.loop_count();
    let (bit_depth, _, icc) = source_meta(&mut decoder);

    animated(
        decoder.into_frames(),
        path,
        ImageFormat::Gif,
        cycles,
        bit_depth,
        icc,
        opts,
    )
}

/// PNG and APNG share a decoder, so the animation is found and the still path
/// taken when there is not one.
fn load_png<R: BufRead + Seek>(
    reader: R,
    path: &Path,
    opts: &LoadOptions,
    limits: Limits,
) -> Result<Decoded, LoadError> {
    let mut decoder =
        PngDecoder::with_limits(reader, limits).map_err(|source| classify(path, source, opts))?;
    let (bit_depth, orientation, icc) = source_meta(&mut decoder);

    // The APNG decoder turns 16-bit frames away rather than scaling them, so
    // those files stay stills instead of failing outright.
    let plays = decoder.is_apng().unwrap_or(false)
        && !matches!(
            decoder.color_type(),
            ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16
        );
    if !plays {
        return still_from(decoder, path, bit_depth, orientation, icc, opts);
    }

    check_area(
        path,
        decoder.dimensions(),
        opts.max_animation_pixels,
        "limits.max_animation_pixels",
    )?;
    let apng = decoder
        .apng()
        .map_err(|source| classify(path, source, opts))?;
    let cycles = apng.loop_count();

    animated(
        apng.into_frames(),
        path,
        ImageFormat::Png,
        cycles,
        bit_depth,
        icc,
        opts,
    )
}

fn load_still<R: BufRead + Seek>(
    reader: ImageReader<R>,
    path: &Path,
    opts: &LoadOptions,
) -> Result<Decoded, LoadError> {
    let mut decoder = reader
        .into_decoder()
        .map_err(|source| classify(path, source, opts))?;
    let (bit_depth, orientation, icc) = source_meta(&mut decoder);

    still_from(decoder, path, bit_depth, orientation, icc, opts)
}

/// Decodes one image out of an already-open decoder.
fn still_from(
    decoder: impl ImageDecoder,
    path: &Path,
    source_bit_depth: u8,
    orientation: Orientation,
    icc_profile: Option<Vec<u8>>,
    opts: &LoadOptions,
) -> Result<Decoded, LoadError> {
    check_area(
        path,
        decoder.dimensions(),
        opts.max_pixels,
        "limits.max_pixels",
    )?;
    let decoded =
        DynamicImage::from_decoder(decoder).map_err(|source| classify(path, source, opts))?;

    from_dynamic_image(
        decoded,
        path,
        source_bit_depth,
        orientation,
        icc_profile,
        opts,
    )
    .map(Decoded::Still)
}

fn classify(path: &Path, source: ImageError, opts: &LoadOptions) -> LoadError {
    match source {
        // The decoder reports which guard tripped, so name the budget behind
        // it: both are configurable and an "Image size exceeds limit" on its
        // own is not something a user can act on.
        ImageError::Limits(limits) => {
            let reason = match limits.kind() {
                image::error::LimitErrorKind::DimensionError => match opts.max_dimension {
                    Some(edge) => {
                        format!("an edge over the configured {edge} pixels (limits.max_dimension)")
                    }
                    None => "an edge over the decoder's own limit".to_string(),
                },
                image::error::LimitErrorKind::InsufficientMemory => {
                    format!("over limits.max_pixels = {}", opts.max_pixels)
                }
                _ => ImageError::Limits(limits).to_string(),
            };
            LoadError::limits(path, reason)
        }
        other => LoadError::Decode {
            path: path.display().to_string(),
            source: other,
        },
    }
}

/// Caller passes what `DynamicImage` drops: bit depth, orientation, ICC.
fn from_dynamic_image(
    mut img: DynamicImage,
    path: &Path,
    source_bit_depth: u8,
    orientation: Orientation,
    icc_profile: Option<Vec<u8>>,
    opts: &LoadOptions,
) -> Result<WorkingImage, LoadError> {
    let cicp = img.color_space();

    // Orient first to save a full-buffer pass after conversion.
    if orientation != Orientation::NoTransforms {
        img.apply_orientation(orientation);
    }

    let width = img.width();
    let height = img.height();

    // Dropped here so the decoded pixels are gone before the transform
    // allocates scratch for the widened image.
    let mut data = to_rgba_f32(&img);
    drop(img);

    let metadata = ColorMetadata::detect(icc_profile, cicp);
    ToLinear::new(&metadata, opts.cms_scratch).apply(&mut data);

    WorkingImage::from_rgba(width, height, data, path, Some(source_bit_depth))
        .ok_or_else(|| LoadError::limits(path, "the image has no pixels"))
}

/// Rejects `width` x `height` above `limit` pixels, before anything is
/// allocated for it. The decoder's own `max_alloc` is a byte budget on its
/// buffers; it does not bound the f32 working copy made afterwards.
fn check_area(
    path: &Path,
    (width, height): (u32, u32),
    limit: u64,
    name: &str,
) -> Result<(), LoadError> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limit {
        return Err(LoadError::limits(
            path,
            format!("{width}x{height} is over {name} = {limit} pixels"),
        ));
    }
    Ok(())
}
