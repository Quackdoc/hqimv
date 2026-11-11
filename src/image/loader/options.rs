//! Decode budgets and timing.

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Reject images above this pixel count.
    pub max_pixels: u64,
    /// Reject animations with more frames than this.
    pub max_animation_frames: usize,
    /// Reject animations whose frames add up to more pixels than this.
    pub max_animation_pixels: u64,
    /// Reject either edge of a single image above this many pixels.
    ///
    /// `max_pixels` bounds area and this bounds shape, and they catch different
    /// things: a 1 x 4-billion header is inside any sane area budget as 8-bit
    /// pixels, then asks for 68 GB once widened to `f32`. Unset means no edge
    /// cap, leaving only the area budget.
    pub max_dimension: Option<u32>,
    /// Cap on one remote response body, in bytes.
    pub max_download_bytes: u64,
    /// Ceiling on the colour conversion's scratch memory, in bytes. A smaller
    /// one means more, smaller passes over a large image.
    pub cms_scratch: usize,
    /// Whole-request budget for a remote image: connect, transfer and each read.
    pub download_timeout: Duration,
    /// Floor for a frame the file marks as instant. Without it a zero-delay
    /// animation would ask the loop for a new frame on every iteration.
    pub min_frame_delay: Duration,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            // 500 MP is a fairly reasonable default.
            max_pixels: 500_000_000,
            // Frames are held decoded, so this pair is the real memory cost of
            // letting an animation play.
            max_animation_frames: 10_000,
            max_animation_pixels: 50_000_000,
            max_dimension: None,
            max_download_bytes: 256 * 1024 * 1024,
            cms_scratch: 32 << 20,
            download_timeout: Duration::from_secs(30),
            min_frame_delay: Duration::from_millis(10),
        }
    }
}
