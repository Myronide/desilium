//! wlr-layer-shell surfaces (panels, docks, launchers, lock screens).

use super::elements::DesiliumElement;
use super::scene::SceneContext;
use crate::state::layers::{ABOVE_WINDOWS, BELOW_WINDOWS, layer_origin};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{LayerMap, LayerSurface, PopupManager, layer_map_for_output};
use smithay::utils::{Logical, Physical, Point};
use smithay::wayland::shell::wlr_layer::Layer;

const OPAQUE: f32 = 1.0;

/// Overlay and top layers: drawn above all windows (but below overlays and the cursor).
pub fn push_above(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
) {
    push_layers(out, renderer, ctx, &ABOVE_WINDOWS);
}

/// Background and bottom layers: drawn below all windows, above the wallpaper.
pub fn push_below(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
) {
    push_layers(out, renderer, ctx, &BELOW_WINDOWS);
}

/// Pushes the given layers topmost first: layers in list order, newest surface first within a layer.
fn push_layers(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    layers: &[Layer],
) {
    let Some(output) = ctx.state.layer_output() else {
        return;
    };
    let map = layer_map_for_output(&output);
    for &layer in layers {
        for surface in map.layers_on(layer).rev() {
            push_layer_surface(out, renderer, ctx, &map, surface);
        }
    }
}

fn push_layer_surface(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    map: &LayerMap,
    surface: &LayerSurface,
) {
    let Some(origin) = layer_origin(map, surface) else {
        return;
    };
    push_popups(out, renderer, ctx, origin, surface);
    let location = to_physical(origin, ctx.scale);
    let tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
        renderer,
        surface.wl_surface(),
        location,
        ctx.scale,
        OPAQUE,
        Kind::Unspecified,
    );
    out.extend(tree.into_iter().map(DesiliumElement::from));
}

/// Popups of a layer surface (menus, tooltips) are drawn above their parent.
fn push_popups(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    origin: Point<i32, Logical>,
    surface: &LayerSurface,
) {
    for (popup, offset) in PopupManager::popups_for_surface(surface.wl_surface()) {
        let position: Point<i32, Logical> = origin + offset - popup.geometry().loc;
        let elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(
                renderer,
                popup.wl_surface(),
                to_physical(position, ctx.scale),
                ctx.scale,
                OPAQUE,
                Kind::Unspecified,
            );
        out.extend(elements.into_iter().map(DesiliumElement::from));
    }
}

fn to_physical(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_physical_precise_round(scale)
}
