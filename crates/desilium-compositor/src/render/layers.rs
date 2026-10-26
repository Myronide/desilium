//! wlr-layer-shell surfaces (panels, docks, launchers, lock screens).

use super::effects::{BackdropBlurParams, Corners};
use super::elements::DesiliumElement;
use super::glass::{self, GlassKey};
use super::popups;
use super::scene::SceneContext;
use crate::state::layers::{ABOVE_WINDOWS, BELOW_WINDOWS, SurfaceEffects, layer_origin};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{LayerMap, LayerSurface, layer_map_for_output};
use smithay::utils::{Logical, Physical, Point, Rectangle};
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
    popups::push(out, renderer, ctx, origin, surface.wl_surface());
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
    request_glass(ctx, map, surface, out.len());
}

/// Records the backdrop blur behind `surface`, right below the elements pushed so far.
fn request_glass(ctx: &SceneContext<'_>, map: &LayerMap, surface: &LayerSurface, index: usize) {
    let effects = ctx
        .state
        .layer_surface_effects(surface.wl_surface())
        .or_else(|| glass::debug::layer_effects(ctx));
    let Some(rect) = map.layer_geometry(surface) else {
        return;
    };
    let theme = ctx.state.theme();
    let Some(params) = glass_params(effects, rect, ctx.scale, theme.effects.backdrop_saturation)
    else {
        return;
    };
    ctx.resources
        .glass
        .request(GlassKey::surface(surface.wl_surface()), index, params);
}

/// Blur parameters for a layer surface at `rect`; `None` without a usable blur request.
fn glass_params(
    effects: Option<SurfaceEffects>,
    rect: Rectangle<i32, Logical>,
    scale: f64,
    saturation: f32,
) -> Option<BackdropBlurParams> {
    let effects = effects.filter(|effects| effects.blur_radius > 0.0)?;
    if rect.is_empty() {
        return None;
    }
    let [top_left, top_right, bottom_right, bottom_left] = effects.corner_radii;
    let corners = Corners {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    };
    Some(
        BackdropBlurParams::new(rect, effects.blur_radius)
            .scale(scale)
            .corners(corners)
            .saturation(saturation),
    )
}

fn to_physical(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_physical_precise_round(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rectangle<i32, Logical> {
        Rectangle::new((10, 20).into(), (300, 40).into())
    }

    fn effects(radius: f32) -> Option<SurfaceEffects> {
        Some(SurfaceEffects {
            blur_radius: radius,
            corner_radii: [1.0, 2.0, 3.0, 4.0],
        })
    }

    #[test]
    fn no_effects_or_no_radius_means_no_glass() {
        assert!(glass_params(None, rect(), 1.0, 1.0).is_none());
        assert!(glass_params(effects(0.0), rect(), 1.0, 1.0).is_none());
        assert!(glass_params(effects(-3.0), rect(), 1.0, 1.0).is_none());
    }

    #[test]
    fn empty_surface_gets_no_glass() {
        let empty = Rectangle::new((0, 0).into(), (0, 40).into());
        assert!(glass_params(effects(24.0), empty, 1.0, 1.0).is_none());
    }

    #[test]
    fn glass_follows_the_surface_rect_corners_and_scale() {
        let params = glass_params(effects(24.0), rect(), 1.5, 1.35);
        let params = params.expect("blur requested");
        assert_eq!(params.rect, rect());
        assert_eq!(params.radius, 24.0);
        assert_eq!(params.scale, 1.5);
        assert_eq!(params.saturation, 1.35);
        assert_eq!(params.corners.top_left, 1.0);
        assert_eq!(params.corners.top_right, 2.0);
        assert_eq!(params.corners.bottom_right, 3.0);
        assert_eq!(params.corners.bottom_left, 4.0);
    }
}
