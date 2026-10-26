//! Where the shell's dock icon of a window is, as target of the minimize animation.

use super::DesiliumState;
use crate::state::layers::layer_origin;
use desilium_wm::{Rect, WindowId};
use smithay::desktop::layer_map_for_output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

/// A rectangle relative to a shell layer surface; resolved lazily so it follows the surface.
#[derive(Debug, Clone)]
pub struct MinimizeTarget {
    pub surface: WlSurface,
    pub rect: Rect,
}

/// Moves `rect` by the position of its surface origin, without overflowing.
pub fn translate(rect: Rect, origin_x: i32, origin_y: i32) -> Rect {
    Rect::new(
        rect.loc.x.saturating_add(origin_x),
        rect.loc.y.saturating_add(origin_y),
        rect.size.w,
        rect.size.h,
    )
}

impl DesiliumState {
    /// Where the dock icon of window `id` is, in output coordinates.
    ///
    /// `None` when the shell gave no rectangle or its layer surface is gone; callers fall back
    /// to the bottom centre of the output.
    #[allow(dead_code)] // consumed by the minimize animation
    pub fn minimize_target(&self, id: WindowId) -> Option<Rect> {
        let target = self.shell.minimize_targets.get(&id)?;
        let layer = self.layer_for_root(&target.surface)?;
        let output = self.layer_output()?;
        let map = layer_map_for_output(&output);
        let origin = layer_origin(&map, &layer)?;
        Some(translate(target.rect, origin.x, origin.y))
    }

    /// Stores or clears the target for `id`; the rectangle is already validated.
    pub(super) fn store_minimize_target(&mut self, id: WindowId, target: Option<MinimizeTarget>) {
        match target {
            Some(target) => {
                self.shell.minimize_targets.insert(id, target);
            }
            None => {
                self.shell.minimize_targets.remove(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_offsets_the_location_only() {
        let moved = translate(Rect::new(10, 5, 40, 30), 100, 700);
        assert_eq!(moved, Rect::new(110, 705, 40, 30));
    }

    #[test]
    fn translate_saturates_instead_of_overflowing() {
        let moved = translate(Rect::new(i32::MAX, i32::MIN, 1, 1), 5, -5);
        assert_eq!(moved.loc.x, i32::MAX);
        assert_eq!(moved.loc.y, i32::MIN);
    }
}
