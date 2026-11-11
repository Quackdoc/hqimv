//! Image loading, buffers, and CPU resampling.

pub mod animation;
pub mod buffer;
pub mod cms;
pub mod loader;
pub mod overlay;
pub mod resample;
pub mod transform;
pub mod widen;

pub use animation::Animation;
pub use buffer::WorkingImage;
pub use loader::{Decoded, LoadOptions};
pub use resample::{Filter, ScaleMode};
