//! The view transform: scale mode, zoom, pan, and the display overlays.

use crate::image::resample::{Filter, ScaleMode, sane_factor};
use crate::settings::Settings;

// 1/64..64x: wide enough to be useful, narrow enough to avoid overflow.
const ZOOM_MIN: f64 = 1.0 / 64.0;
const ZOOM_MAX: f64 = 64.0;

/// What the current file looks like: scale, zoom, pan, and the overlays.
pub struct View {
    pub scale_mode: ScaleMode,
    pub zoom: f64,
    pub offset_x: i32,
    pub offset_y: i32,
    pub background: [f32; 4],
    pub status_line: bool,
    pub info_overlay: bool,
    /// Resampling kernel used when the image does not land at its own size.
    pub filter: Filter,
}

impl View {
    pub fn new(settings: &Settings) -> Self {
        Self {
            scale_mode: settings.scale_mode,
            zoom: settings.zoom,
            offset_x: 0,
            offset_y: 0,
            background: settings.background,
            status_line: settings.status_line,
            info_overlay: settings.info_overlay,
            filter: settings.filter,
        }
    }

    /// Clamped to the supported range; non-finite or non-positive input ignored.
    pub fn zoom_by(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        }
    }

    /// A factor of zero restores the default view.
    pub fn set_zoom(&mut self, factor: f64) {
        if factor == 0.0 {
            self.reset();
        } else {
            self.zoom = sane_factor(factor);
        }
    }

    /// Natural scale, centred.
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.offset_x = 0;
        self.offset_y = 0;
    }

    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.offset_x = self.offset_x.saturating_add(dx);
        self.offset_y = self.offset_y.saturating_add(dy);
    }
}
