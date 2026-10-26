//! Deterministic animation engine for the compositor.
//!
//! Nothing here reads the wall clock or performs I/O. Every query takes the current
//! time as a [`Duration`] since an arbitrary epoch chosen by the caller, so the same
//! inputs always produce the same frames and tests never sleep.

#![forbid(unsafe_code)]

mod animator;
mod easing;
mod lerp;
mod presets;
mod spring;
mod tween;

pub use animator::Animator;
pub use easing::Easing;
pub use lerp::Lerp;
pub use presets::{WINDOW_SCALE_FROM, close_scale_alpha, open_scale_alpha};
pub use spring::{Spring, SpringState};
pub use tween::Tween;
