//! Wallpaper decoding, fit math, rendering and procedural gradients.
//!
//! Pure CPU code with no windowing dependencies. Every pixel output is a tightly packed RGBA8
//! buffer, ready for texture upload.

mod color;
mod error;
mod geometry;
mod gradient;
mod load;
mod mesh;
mod render;
mod resample;

pub use color::average_color;
pub use error::WallpaperError;
pub use geometry::{FitMode, Placement, RectF, RectI, Size, fit_placement};
pub use gradient::{GradientStop, render_gradient};
pub use load::{LoadOptions, MAX_EDGE_PX, MAX_FILE_BYTES, Wallpaper, downscale_if_larger};
pub use mesh::{Blob, MeshSpec, PRESET_NAMES, preset, render_mesh};
