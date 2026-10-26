//! CPU text rasterizer for window titles and compositor UI.
//!
//! Produces plain premultiplied RGBA bitmaps. It has no dependency on Smithay or OpenGL, so the
//! compositor uploads the results as textures. Fonts come from system discovery (fontdb) and are
//! rasterized with fontdue; glyph masks are cached per (face, glyph, size).

#![forbid(unsafe_code)]

mod compose;
mod font;
mod glyph;
mod layout;
mod renderer;
mod types;

pub use compose::COVERAGE_GAMMA;
pub use layout::{MAX_CHARS, MAX_LINE_WIDTH_PX};
pub use renderer::TextRenderer;
pub use types::{
    Bitmap, CacheStats, FontStack, MAX_RASTER_PX, MAX_SCALE, Size, TextError, TextStyle, Weight,
};
