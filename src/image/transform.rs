//! Rotation and mirroring on raw interleaved samples.
//!
//! Shared by still buffers and animation frames so both do it with one copy
//! of the loops.

use rayon::prelude::*;

/// One rotation or mirror, named once so a key press can apply it to a still
/// buffer or to every frame of an animation.
#[derive(Debug, Clone, Copy)]
pub enum Turn {
    RotateCw,
    RotateCcw,
    FlipHorizontal,
    FlipVertical,
}

impl Turn {
    /// True when the operation trades width for height.
    pub fn swaps_extents(self) -> bool {
        matches!(self, Self::RotateCw | Self::RotateCcw)
    }

    /// Rewrites `data` in place; `w` and `h` are its extents going in.
    pub fn apply<S: Copy + Default + Send + Sync>(
        self,
        w: usize,
        h: usize,
        data: &mut Vec<S>,
        ch: usize,
    ) {
        // Each arm maps an output pixel (row, col) to its source pixel index.
        let src = &*data;
        let mut out = vec![S::default(); src.len()];
        match self {
            Self::RotateCw => gather(src, &mut out, h, ch, |y, x| (h - 1 - x) * w + y),
            Self::RotateCcw => gather(src, &mut out, h, ch, |y, x| x * w + (w - 1 - y)),
            Self::FlipHorizontal => gather(src, &mut out, w, ch, |y, x| y * w + (w - 1 - x)),
            Self::FlipVertical => gather(src, &mut out, w, ch, |y, x| (h - 1 - y) * w + x),
        }
        *data = out;
    }
}

/// Fills `out` (rows of `out_w` pixels) from `src` through `index`.
fn gather<S: Copy + Send + Sync>(
    src: &[S],
    out: &mut [S],
    out_w: usize,
    ch: usize,
    index: impl Fn(usize, usize) -> usize + Sync,
) {
    out.par_chunks_mut(out_w * ch)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, dst) in row.chunks_exact_mut(ch).enumerate() {
                let s = index(y, x) * ch;
                dst.copy_from_slice(&src[s..s + ch]);
            }
        });
}
