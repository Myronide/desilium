//! Procedural "mesh gradient" images: soft overlapping glows over a deep base, with film grain.
//!
//! Everything is a pure function of the spec, the size and the seed, so generated wallpapers are
//! reproducible byte for byte.

mod noise;
mod presets;
mod render;

pub use presets::{PRESET_NAMES, preset};
pub use render::render_mesh;

/// One soft glow in a mesh gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    /// Center as fractions of the image width and height.
    pub center: (f32, f32),
    /// Radius as a fraction of the image height.
    pub radius: f32,
    /// Elongation along `angle_deg`; `1.0` is a circle.
    pub stretch: f32,
    /// Direction of the long axis in degrees.
    pub angle_deg: f32,
    /// sRGB color of the glow.
    pub color: [u8; 3],
    /// Peak opacity at the center, `0..=1`.
    pub strength: f32,
}

/// Recipe for one generated wallpaper.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshSpec {
    /// Base colors at the top and the bottom edge (sRGB).
    pub base: [[u8; 3]; 2],
    /// Glows, painted in order.
    pub blobs: Vec<Blob>,
    /// How far the glows are bent by smooth noise, as a fraction of the image height.
    pub warp: f32,
    /// Edge darkening, `0..=1`.
    pub vignette: f32,
    /// Film grain amplitude in 8-bit levels (standard deviation).
    pub grain: f32,
}
