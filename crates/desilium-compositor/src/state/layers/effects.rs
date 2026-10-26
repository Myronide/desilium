//! Compositor-side effects requested for layer surfaces (blur behind panels and docks).

use crate::state::DesiliumState;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use std::collections::HashMap;

/// Largest blur radius or corner radius (logical px) the compositor honours.
pub const MAX_EFFECT_PX: u32 = 64;

/// Effects the shell asked the compositor to apply behind one of its layer surfaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceEffects {
    /// Backdrop blur radius in logical px (0 = none).
    pub blur_radius: f32,
    /// Corner radii in logical px: top-left, top-right, bottom-right, bottom-left.
    pub corner_radii: [f32; 4],
}

impl SurfaceEffects {
    /// Builds effects from untrusted client values, clamping each to `0..=MAX_EFFECT_PX`.
    pub fn from_request(blur_radius: u32, corner_radii: [u32; 4]) -> Self {
        Self {
            blur_radius: clamp_px(blur_radius),
            corner_radii: corner_radii.map(clamp_px),
        }
    }

    /// True when nothing would be drawn.
    pub fn is_empty(&self) -> bool {
        self.blur_radius <= 0.0 && self.corner_radii.iter().all(|radius| *radius <= 0.0)
    }
}

fn clamp_px(value: u32) -> f32 {
    let clamped = u16::try_from(value.min(MAX_EFFECT_PX)).unwrap_or(u16::MAX);
    f32::from(clamped)
}

/// Effects per layer surface root; entries die with their surface.
#[derive(Debug, Default)]
pub struct LayerEffects {
    by_surface: HashMap<WlSurface, SurfaceEffects>,
}

impl LayerEffects {
    /// Stores the effects of `surface`; empty effects remove the entry.
    pub fn set(&mut self, surface: &WlSurface, effects: SurfaceEffects) {
        if effects.is_empty() {
            self.by_surface.remove(surface);
            return;
        }
        self.by_surface.insert(surface.clone(), effects);
    }

    /// Effects of `surface`, if any were requested.
    pub fn get(&self, surface: &WlSurface) -> Option<SurfaceEffects> {
        self.by_surface.get(surface).copied()
    }

    /// Forgets surfaces that were destroyed.
    pub fn prune(&mut self) {
        self.by_surface.retain(|surface, _| surface.is_alive());
    }

    /// True when no effects are stored.
    pub fn is_empty(&self) -> bool {
        self.by_surface.is_empty()
    }
}

impl DesiliumState {
    /// Effects configured for `surface` (filled in by the shell protocol, consumed by the renderer).
    pub fn layer_surface_effects(&self, surface: &WlSurface) -> Option<SurfaceEffects> {
        self.shell_effects().get(surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_inside_the_range_pass_through() {
        let effects = SurfaceEffects::from_request(24, [8, 0, 12, 64]);
        assert_eq!(effects.blur_radius, 24.0);
        assert_eq!(effects.corner_radii, [8.0, 0.0, 12.0, 64.0]);
    }

    #[test]
    fn oversized_values_are_clamped() {
        let effects = SurfaceEffects::from_request(u32::MAX, [65, 1000, u32::MAX, 64]);
        assert_eq!(effects.blur_radius, 64.0);
        assert_eq!(effects.corner_radii, [64.0; 4]);
    }

    #[test]
    fn all_zero_effects_are_empty() {
        assert!(SurfaceEffects::from_request(0, [0; 4]).is_empty());
        assert!(!SurfaceEffects::from_request(0, [0, 0, 0, 1]).is_empty());
        assert!(!SurfaceEffects::from_request(1, [0; 4]).is_empty());
    }
}
