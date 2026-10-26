//! Backdrop blur ("frosted glass") for translucent surfaces.
//!
//! [`BackdropBlur`] is a render element that, when drawn, copies the part of the frame
//! buffer below it (everything drawn earlier in the same frame), blurs it and draws the
//! result clipped to a rounded rectangle, optionally saturated, tinted and dithered.
//!
//! # Pipeline
//!
//! 1. `glCopyTexSubImage2D` of the glass rectangle plus a margin of 3 sigma (so content just
//!    outside the glass bleeds in naturally; at screen edges the colour is extended).
//! 2. N dual-Kawase style 2x down-samples (a 4x4 tent footprint, no aliasing) until the
//!    remaining sigma is at most 3.5 texels (N = 0 for tiny radii, 3 for 48 px at scale 1).
//! 3. Separable Gaussian at that level with linear-sampling tap pairs (17 fetches per pass).
//! 4. Composite with cubic B-spline up-sampling, a rounded-rect coverage mask, saturation,
//!    tint over the blur and +-1 LSB noise.
//!
//! Everything works in framebuffer pixels derived from the frame's projection matrix, so any
//! `Transform` (`Normal`, the `Flipped180` of the winit output, rotations) is handled. All
//! textures are RGBA8, clamp-to-edge, and the valid region is clamped inside the shaders, so
//! no garbage from over-sized textures can leak in.
//!
//! # Integration
//!
//! ```ignore
//! let effects = Effects::compile(renderer)?;                // once; also compiles the blur
//! let mut slot = BlurSlot::default();                        // one per glass surface
//! // every frame, after building the other elements of the scene (topmost first):
//! if let Some(shared) = effects.backdrop_blur_resources() {
//!     let params = BackdropBlurParams::new(panel_rect, theme.effects.blur_radius)
//!         .scale(scale).corners(Corners::all(12.0)).tint(panel_fill);
//!     let glass = slot.update(shared, params, Scale::from(scale), others.iter());
//!     others.insert(index_just_above_the_content_it_blurs, glass.into());
//! }
//! ```
//!
//! The translucent surface itself (client buffer, text) stays an ordinary element above the
//! glass; if the fill is a flat colour pass it as `tint` and skip a separate rect.
//!
//! # Damage strategy (conservative and simple)
//!
//! The glass repaints as a whole whenever any other element within reach (the glass area
//! plus the blur margin) was added, removed, moved or committed. [`BackdropWatch`] detects
//! this from element ids, commit counters and geometry; the glass then bumps its commit and
//! reports full-area damage, so the tracker re-renders everything below it first. A glass
//! over a static desktop therefore costs no GPU work at all.

mod element;
mod gl;
mod params;
mod plan;
mod resources;

#[cfg(test)]
mod gpu_tests;

pub use element::{BackdropBlur, BackdropWatch, BlurSlot};
pub use gl::BlurError;
pub use params::BackdropBlurParams;
pub use resources::SharedBlur;
