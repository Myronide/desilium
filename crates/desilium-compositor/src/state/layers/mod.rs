//! wlr-layer-shell bookkeeping: arrangement, work area, keyboard and pointer routing.

mod effects;
mod policy;
mod workarea;

pub(crate) use effects::LayerEffects;
#[allow(unused_imports)] // consumed by the renderer
pub use effects::SurfaceEffects;
pub use policy::{ABOVE_WINDOWS, BELOW_WINDOWS};

use super::DesiliumState;
use policy::pick_exclusive;
use smithay::desktop::{LayerMap, LayerSurface, WindowSurfaceType, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, Layer};
use std::time::Duration;
use workarea::{moved_windows, work_area_of};

/// A layer surface (or one of its popups or subsurfaces) under a pointer position.
pub struct LayerHit {
    /// The mapped layer surface the hit belongs to.
    pub layer: LayerSurface,
    /// The surface that actually received the hit.
    pub surface: WlSurface,
    /// Pointer position relative to the output, adjusted to the hit surface.
    pub location: Point<f64, Logical>,
}

impl DesiliumState {
    /// The only output in this compositor; layer surfaces are arranged on it.
    pub fn layer_output(&self) -> Option<Output> {
        self.space.outputs().next().cloned()
    }

    /// Re-arranges all layer surfaces and publishes the free area as the work area.
    ///
    /// Call after any layer commit, layer destruction or output resize.
    pub fn arrange_layers(&mut self) {
        let Some(output) = self.layer_output() else {
            return;
        };
        let zone = {
            let mut map = layer_map_for_output(&output);
            map.arrange();
            map.non_exclusive_zone()
        };
        self.set_work_area(work_area_of(zone));
    }

    /// Applies a new work area; maximized windows follow it and get a fresh configure.
    fn set_work_area(&mut self, area: desilium_wm::Rect) {
        if self.wm.work_area() == area {
            return;
        }
        // Every desktop follows the work area, so the snapshot spans all of them: a maximized
        // window on a hidden desktop must be configured too.
        let before = self.wm.all_placements();
        self.wm.set_work_area(area);
        let after = self.wm.all_placements();
        for id in moved_windows(&before, &after) {
            self.configure_window(id);
        }
        self.sync_space();
    }

    /// Removes a destroyed layer surface from the layer map; true when it was mapped.
    pub fn unmap_layer_surface(
        &mut self,
        surface: &smithay::wayland::shell::wlr_layer::LayerSurface,
    ) -> bool {
        let Some(output) = self.layer_output() else {
            return false;
        };
        let found = layer_map_for_output(&output)
            .layers()
            .find(|layer| layer.layer_surface() == surface)
            .cloned();
        let Some(layer) = found else {
            return false;
        };
        layer_map_for_output(&output).unmap_layer(&layer);
        true
    }

    /// The mapped layer surface whose root is `root`, if any.
    pub(crate) fn layer_for_root(&self, root: &WlSurface) -> Option<LayerSurface> {
        let output = self.layer_output()?;
        let map = layer_map_for_output(&output);
        map.layer_for_surface(root, WindowSurfaceType::TOPLEVEL)
            .cloned()
    }

    /// The layer surface that currently demands exclusive keyboard focus.
    pub fn exclusive_layer_surface(&self) -> Option<WlSurface> {
        let output = self.layer_output()?;
        let candidates: Vec<(Layer, KeyboardInteractivity, LayerSurface)> =
            layer_map_for_output(&output)
                .layers()
                .map(|layer| {
                    let mode = layer.cached_state().keyboard_interactivity;
                    (layer.layer(), mode, layer.clone())
                })
                .collect();
        pick_exclusive(candidates).map(|layer| layer.wl_surface().clone())
    }

    /// Hit-tests the given layers, topmost first, and returns the surface under `location`.
    pub(crate) fn layer_surface_under(
        &self,
        layers: &[Layer],
        location: Point<f64, Logical>,
    ) -> Option<LayerHit> {
        let output = self.layer_output()?;
        let map = layer_map_for_output(&output);
        for &kind in layers {
            for layer in map.layers_on(kind).rev() {
                let Some(origin) = layer_origin(&map, layer) else {
                    continue;
                };
                let local = location - origin.to_f64();
                if let Some((surface, offset)) = layer.surface_under(local, WindowSurfaceType::ALL)
                {
                    return Some(LayerHit {
                        layer: layer.clone(),
                        surface,
                        location: (origin + offset).to_f64(),
                    });
                }
            }
        }
        None
    }

    /// Click-to-focus for overlay and top layers; returns true when the click belongs to a layer.
    ///
    /// Background and bottom layers never take the click's focus, so clicking the desktop
    /// still clears window focus as before.
    pub(crate) fn focus_layer_at(&mut self, location: Point<f64, Logical>) -> bool {
        let Some(hit) = self.layer_surface_under(&ABOVE_WINDOWS, location) else {
            return false;
        };
        let may_focus = self.exclusive_layer_surface().is_none();
        if may_focus && hit.layer.can_receive_keyboard_focus() {
            self.set_keyboard_focus(Some(hit.layer.wl_surface().clone()));
        }
        true
    }

    /// Keeps keyboard focus in step with exclusive layers after a commit.
    ///
    /// Only steps in when the focused surface no longer matches the exclusive one or a
    /// focused layer dropped its keyboard interest; on-demand layers keep their focus.
    pub(crate) fn sync_keyboard_focus(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let focused = keyboard.current_focus();
        let exclusive = self.exclusive_layer_surface();
        let stale = match &exclusive {
            Some(surface) => focused.as_ref() != Some(surface),
            None => focused
                .as_ref()
                .is_some_and(|surface| self.layer_refuses_focus(surface)),
        };
        if stale {
            self.apply_focus();
        }
    }

    fn layer_refuses_focus(&self, surface: &WlSurface) -> bool {
        self.layer_for_root(surface)
            .is_some_and(|layer| !layer.can_receive_keyboard_focus())
    }

    fn set_keyboard_focus(&mut self, surface: Option<WlSurface>) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
        self.request_redraw();
    }

    /// Frame callbacks for layer surfaces, so bars and docks can keep animating.
    pub fn send_layer_frame_callbacks(&self, output: &Output, elapsed: Duration) {
        let map = layer_map_for_output(output);
        for layer in map.layers() {
            layer.send_frame(output, elapsed, Some(Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
}

/// Where a layer surface's own surface origin sits in output coordinates.
///
/// Smithay reports the bounding box, which can start left of or above the origin when
/// subsurfaces stick out; the origin is the box position minus its own offset.
pub(crate) fn layer_origin(map: &LayerMap, layer: &LayerSurface) -> Option<Point<i32, Logical>> {
    let geometry = map.layer_geometry(layer)?;
    Some(geometry.loc - layer.bbox().loc)
}
