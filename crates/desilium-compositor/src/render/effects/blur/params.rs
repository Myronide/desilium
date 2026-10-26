//! Parameters of a backdrop blur surface.

use super::super::types::{Corners, Rgba};
use smithay::utils::{Logical, Rectangle};

/// Default saturation boost of the backdrop (a touch above neutral reads as "glass").
const DEFAULT_SATURATION: f32 = 1.0;
/// Noise amplitude in 8 bit steps; +-1 LSB triangular noise hides banding.
const DEFAULT_NOISE: f32 = 1.0;

/// Describes one frosted-glass surface.
#[derive(Debug, Clone, PartialEq)]
pub struct BackdropBlurParams {
    /// Glass area in output-global logical coordinates.
    pub rect: Rectangle<i32, Logical>,
    /// Blur radius in logical px (theme `effects.blur_radius`); about two Gaussian sigma.
    pub radius: f32,
    /// Output scale; scales the radius and corner radii into physical px.
    pub scale: f64,
    /// Corner radii in logical px.
    pub corners: Corners,
    /// Backdrop saturation multiplier (`1.0` = unchanged, theme `backdrop_saturation`).
    pub saturation: f32,
    /// Premultiplied tint composited over the blurred backdrop (the surface fill).
    pub tint: Rgba,
    /// Overall opacity of the glass, e.g. for fade animations.
    pub opacity: f32,
    /// Noise amplitude in 8 bit steps (`0.0` disables it).
    pub noise: f32,
}

impl BackdropBlurParams {
    /// Plain blur of `rect` with `radius` logical px at scale 1 and no tint.
    pub fn new(rect: Rectangle<i32, Logical>, radius: f32) -> Self {
        BackdropBlurParams {
            rect,
            radius,
            scale: 1.0,
            corners: Corners::NONE,
            saturation: DEFAULT_SATURATION,
            tint: Rgba::TRANSPARENT,
            opacity: 1.0,
            noise: DEFAULT_NOISE,
        }
    }

    /// Sets the output scale.
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Sets the corner radii (logical px).
    pub fn corners(mut self, corners: Corners) -> Self {
        self.corners = corners;
        self
    }

    /// Sets the saturation multiplier.
    pub fn saturation(mut self, saturation: f32) -> Self {
        self.saturation = saturation;
        self
    }

    /// Sets the premultiplied tint over the blurred backdrop.
    pub fn tint(mut self, tint: Rgba) -> Self {
        self.tint = tint;
        self
    }

    /// Sets the overall opacity.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Sets the noise amplitude.
    pub fn noise(mut self, noise: f32) -> Self {
        self.noise = noise;
        self
    }
}
