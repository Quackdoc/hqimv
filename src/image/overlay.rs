//! Text overlay via fontdue into the f32 RGBA buffer.

use std::sync::OnceLock;

use super::buffer::CHANNELS;

static FONT: OnceLock<fontdue::Font> = OnceLock::new();

fn font() -> &'static fontdue::Font {
    FONT.get_or_init(|| {
        fontdue::Font::from_bytes(
            include_bytes!("../../assets/fonts/DejaVuSans.ttf").as_slice(),
            fontdue::FontSettings::default(),
        )
        .expect("bundled font parses")
    })
}

struct LaidGlyph {
    metrics: fontdue::Metrics,
    bitmap: Vec<u8>,
    pen_x: f32,
}

/// Line box height for `font_px`, from font metrics alone — no glyph
/// rasterising, so this is the cheap way to stack lines before committing to
/// an overlay.
pub fn line_height(font_px: f32) -> u32 {
    let (ascent, descent) = line_metrics(font_px);
    (ascent - descent).ceil().max(0.0) as u32
}

/// Ascent and descent for `font_px`, from font metrics alone.
fn line_metrics(font_px: f32) -> (f32, f32) {
    font()
        .horizontal_line_metrics(font_px)
        .map(|m| (m.ascent, m.descent))
        .unwrap_or((font_px, 0.0))
}

/// Rasterises every glyph of `text` at `font_px`, returning them, the ascent,
/// and the line height.
fn layout(text: &str, font_px: f32) -> (Vec<LaidGlyph>, f32, f32) {
    let font = font();
    let mut pen_x = 0.0f32;
    let mut glyphs = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, font_px);
        glyphs.push(LaidGlyph {
            metrics,
            bitmap,
            pen_x,
        });
        pen_x += metrics.advance_width;
    }
    let (ascent, descent) = line_metrics(font_px);
    (glyphs, ascent, ascent - descent)
}

/// Window-space text, drawn after blit so pan/zoom never move it.
///
/// The glyphs are rasterised when this is built and reused for both measuring
/// and drawing.
pub struct TextOverlay {
    glyphs: Vec<LaidGlyph>,
    ascent: f32,
    text_width: u32,
    text_height: u32,
    font_px: f32,
    /// Left and top edge of the line box, in window coordinates.
    left: i32,
    top: i32,
}

impl TextOverlay {
    /// `(left, top)` is the line-box top-left, in window coordinates.
    pub fn new(text: &str, font_px: f32, left: i32, top: i32) -> Self {
        let (glyphs, ascent, line_height) = layout(text, font_px);
        let text_width = glyphs
            .last()
            .map(|g| g.pen_x + g.metrics.advance_width)
            .unwrap_or(0.0);
        Self {
            glyphs,
            ascent,
            text_width: text_width.ceil().max(0.0) as u32,
            text_height: line_height.ceil().max(0.0) as u32,
            font_px,
            left,
            top,
        }
    }

    /// Laid-out size in pixels, for stacking lines.
    pub fn size(&self) -> (u32, u32) {
        (self.text_width, self.text_height)
    }

    pub fn position(&self) -> (i32, i32) {
        (self.left, self.top)
    }

    /// White text with a black outline, into an RGBA buffer.
    ///
    /// `(left, top)` is the line-box top-left to draw at, in `data`
    /// coordinates. That is the overlay's own position shifted into the
    /// clipped region, which is what `blit_text_overlay` passes; it is not
    /// added to this overlay's position again.
    pub fn draw(
        &self,
        data: &mut [f32],
        canvas_width: u32,
        canvas_height: u32,
        left: i32,
        top: i32,
    ) {
        let mut canvas = Canvas {
            data,
            width: canvas_width as i32,
            height: canvas_height as i32,
        };
        let baseline = top as f32 + self.ascent;
        let outline_radius = ((self.font_px / 24.0).round() as i32).max(1);
        for glyph in &self.glyphs {
            let metrics = &glyph.metrics;
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }
            let glyph_left = (left as f32 + glyph.pen_x).round() as i32 + metrics.xmin;
            let glyph_top = (baseline - metrics.ymin as f32 - metrics.height as f32).round() as i32;
            // The outline is the glyph stamped in black at every offset in a
            // square around its real position; the white glyph goes on top.
            for dy in -outline_radius..=outline_radius {
                for dx in -outline_radius..=outline_radius {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    canvas.stamp(
                        &glyph.bitmap,
                        metrics.width,
                        glyph_left + dx,
                        glyph_top + dy,
                        [0.0, 0.0, 0.0],
                    );
                }
            }
            canvas.stamp(
                &glyph.bitmap,
                metrics.width,
                glyph_left,
                glyph_top,
                [1.0, 1.0, 1.0],
            );
        }
    }
}

/// Destination pixels, bounds-checked once per stamp rather than per index.
struct Canvas<'a> {
    data: &'a mut [f32],
    width: i32,
    height: i32,
}

impl Canvas<'_> {
    /// Blends a coverage bitmap `bitmap_width` pixels wide onto the canvas
    /// with its top-left at `(left, top)`, clipping to the canvas.
    fn stamp(&mut self, bitmap: &[u8], bitmap_width: usize, left: i32, top: i32, color: [f32; 3]) {
        for (index, &coverage) in bitmap.iter().enumerate() {
            if coverage == 0 {
                continue;
            }
            let x = left + (index % bitmap_width) as i32;
            let y = top + (index / bitmap_width) as i32;
            if x < 0 || y < 0 || x >= self.width || y >= self.height {
                continue;
            }
            let alpha = f32::from(coverage) / 255.0;
            let offset = (y as usize * self.width as usize + x as usize) * CHANNELS;
            let pixel = &mut self.data[offset..offset + CHANNELS];
            // Straight-alpha "over": over a transparent pixel the text keeps its
            // own colour and only gains coverage, instead of a black box.
            let dst_weight = pixel[3] * (1.0 - alpha);
            let out_alpha = alpha + dst_weight;
            for channel in 0..3 {
                pixel[channel] = (color[channel] * alpha + pixel[channel] * dst_weight) / out_alpha;
            }
            pixel[3] = out_alpha;
        }
    }
}
