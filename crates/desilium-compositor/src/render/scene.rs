use super::decoration::{Layer, ShadowCaster, View};
use super::effects::{Corners, Rgba, RoundedSurfaceElement};
use super::elements::DesiliumElement;
use super::resources::RenderResources;
use super::transform::NoOpaque;
use super::{background, cursor, decoration, glass, layers, overlay, popups, snapshot};
use crate::animation::Xform;
use crate::decoration::{Chrome, Frame};
use crate::state::{DesiliumState, SceneWindow};
use desilium_theme::{Color, Theme};
use desilium_wm::{Rect, WindowState};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::output::Output;
use smithay::utils::{Logical, Physical, Point, Rectangle};

/// Everything a frame needs besides the renderer.
pub struct SceneContext<'a> {
    pub resources: &'a RenderResources,
    pub state: &'a DesiliumState,
    /// Output area in logical coordinates.
    pub area: Rectangle<i32, Logical>,
    pub scale: f64,
}

impl<'a> SceneContext<'a> {
    /// Derives the logical area and scale from the output's current mode.
    pub fn new(resources: &'a RenderResources, state: &'a DesiliumState, output: &Output) -> Self {
        let scale = output.current_scale().fractional_scale();
        let size = output
            .current_mode()
            .map(|mode| mode.size.to_f64().to_logical(scale).to_i32_round())
            .unwrap_or_default();
        Self {
            resources,
            state,
            area: Rectangle::new((0, 0).into(), size),
            scale,
        }
    }
}

/// Builds the frame as a list of elements, topmost first.
pub fn build(renderer: &mut GlesRenderer, ctx: &SceneContext<'_>) -> Vec<DesiliumElement> {
    let theme = ctx.state.theme();
    let mut elements = Vec::new();
    let resources = ctx.resources;
    resources.decoration.begin_frame();
    resources.popups.begin_frame();
    resources.glass.begin_frame();
    resources.glass.track_output(
        renderer,
        resources.effects.as_ref(),
        ctx.area.size,
        ctx.scale,
    );
    cursor::push(&mut elements, renderer, ctx);
    overlay::push(&mut elements, renderer, ctx);
    layers::push_above(&mut elements, renderer, ctx);
    glass::debug::push_popup(&mut elements, ctx);
    let windows = ctx.state.scene_windows();
    let closing = ctx.state.closing_windows();
    for ghost in &closing {
        snapshot::push_ghost(&mut elements, ctx, ghost);
    }
    for window in windows.iter().rev() {
        let view = View {
            xform: window.xform,
            layer: Layer::Screen,
        };
        push_window(&mut elements, renderer, ctx, theme, window, view);
        capture_if_wanted(renderer, ctx, theme, window);
    }
    let dropped = ctx.resources.snapshots.retain(|id| {
        closing.iter().any(|ghost| ghost.id == id)
            || ctx.state.close_requested(id)
            || windows
                .iter()
                .any(|window| window.id == id && ctx.state.snapshot_wanted(window))
    });
    for id in dropped {
        ctx.state.note_snapshot(id, false);
    }
    resources.decoration.end_frame();
    layers::push_below(&mut elements, renderer, ctx);
    background::push(&mut elements, renderer, ctx);
    resources.popups.end_frame();
    resources
        .glass
        .resolve(resources.effects.as_ref(), ctx.scale, &mut elements);
    elements
}

/// Keeps a fresh snapshot of windows that may close any moment (see `state::animation`).
fn capture_if_wanted(
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    theme: &Theme,
    window: &SceneWindow,
) {
    if !ctx.state.snapshot_wanted(window) {
        return;
    }
    let taken = ctx
        .resources
        .snapshots
        .capture(renderer, ctx, theme, window);
    ctx.state.note_snapshot(window.id, taken);
}

pub fn rgba(color: Color) -> Rgba {
    Rgba::from_straight(color.r, color.g, color.b, color.a)
}

/// Draws one window (shadow, frame, client) through `view`; also used to render snapshots.
pub(super) fn push_window(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    theme: &Theme,
    window: &SceneWindow,
    view: View,
) {
    let Some(toplevel) = window.window.toplevel() else {
        return;
    };
    let geometry = window.window.geometry();
    let frame = &window.frame;
    let xform = view.xform;
    let placement = ClientPlacement::new(&xform, frame, geometry, ctx.scale);
    let radius = theme.window.corner_radius;
    let flat = matches!(
        window.state,
        WindowState::Maximized | WindowState::Fullscreen
    );

    let surface = toplevel.wl_surface();
    // Popups are only drawn while the window rests: they have no snapshot to animate with.
    if xform.is_identity() && view.layer == Layer::Screen {
        let client_origin = (window.frame.client.loc.x, window.frame.client.loc.y).into();
        popups::push(out, renderer, ctx, client_origin, surface);
    }
    decoration::push_chrome(out, renderer, ctx, window, view);
    let tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
        renderer,
        surface,
        placement.origin,
        ctx.scale,
        xform.alpha,
        Kind::Unspecified,
    );
    let corners = scale_corners(
        client_corners(window, radius, flat, ctx),
        placement.radius_factor,
    );
    push_surface_tree(out, ctx, tree, &placement, corners);
    if view.layer != Layer::Screen {
        return;
    }
    let caster = match frame.chrome {
        Chrome::Windowed => frame.outer,
        Chrome::None if !flat => placement.layout,
        Chrome::None | Chrome::Maximized => return,
    };
    let shadow = ShadowCaster {
        id: window.id,
        caster,
        radius,
        focus: window.focus,
        xform,
    };
    decoration::push_shadow(out, ctx, shadow);
}

/// Where the client surface is drawn.
///
/// The surface tree is always built at its natural device-pixel position; while the window is
/// transformed, wrappers scale it around its client origin and move it to the animated spot.
struct ClientPlacement {
    /// The client rectangle in its final layout.
    layout: Rect,
    /// Origin of the surface tree.
    origin: Point<i32, Physical>,
    /// Clip rectangle in the space the rounded shader sees (final device pixels).
    clip: ClipSpace,
    /// Multiplies the corner radii to match the clip space.
    radius_factor: f32,
    /// Scale and move applied to the tree; `None` while the window rests.
    motion: Option<TreeMotion>,
}

/// A rounded-clip rectangle together with the scale that maps it to device pixels.
#[derive(Clone, Copy)]
struct ClipSpace {
    rect: Rectangle<i32, Logical>,
    scale: f64,
}

#[derive(Clone, Copy)]
struct TreeMotion {
    pivot: Point<i32, Physical>,
    ratio: (f64, f64),
    offset: Point<i32, Physical>,
}

impl ClientPlacement {
    fn new(
        xform: &Xform,
        frame: &Frame,
        geometry: Rectangle<i32, Logical>,
        output_scale: f64,
    ) -> ClientPlacement {
        let layout = Rect::new(
            frame.client.loc.x,
            frame.client.loc.y,
            geometry.size.w,
            geometry.size.h,
        );
        let client_origin: Point<i32, Logical> = (layout.loc.x, layout.loc.y).into();
        let origin = (client_origin - geometry.loc).to_physical_precise_round(output_scale);
        if xform.is_identity() {
            return ClientPlacement {
                layout,
                origin,
                clip: ClipSpace {
                    rect: Rectangle::new(client_origin, geometry.size),
                    scale: output_scale,
                },
                radius_factor: 1.0,
                motion: None,
            };
        }
        let device = |value: i32| (f64::from(value) * output_scale).round();
        let moved = xform.rect(layout);
        let (left, top) = (device(moved.loc.x) as i32, device(moved.loc.y) as i32);
        let (width, height) = (device(moved.size.w).max(1.0), device(moved.size.h).max(1.0));
        let pivot: Point<i32, Physical> = client_origin.to_physical_precise_round(output_scale);
        let natural_w = device(geometry.size.w).max(1.0);
        let natural_h = device(geometry.size.h).max(1.0);
        ClientPlacement {
            layout,
            origin,
            // The clip is already in device pixels, so it is converted with scale 1.
            clip: ClipSpace {
                rect: Rectangle::new((left, top).into(), (width as i32, height as i32).into()),
                scale: 1.0,
            },
            radius_factor: xform.uniform() * output_scale as f32,
            motion: Some(TreeMotion {
                pivot,
                ratio: (width / natural_w, height / natural_h),
                offset: (left - pivot.x, top - pivot.y).into(),
            }),
        }
    }
}

fn scale_corners(corners: Corners, factor: f32) -> Corners {
    Corners {
        top_left: corners.top_left * factor,
        top_right: corners.top_right * factor,
        bottom_right: corners.bottom_right * factor,
        bottom_left: corners.bottom_left * factor,
    }
}

/// Client content only rounds the corners the title bar does not own.
fn client_corners(
    window: &SceneWindow,
    radius: f32,
    flat: bool,
    ctx: &SceneContext<'_>,
) -> Corners {
    match window.frame.chrome {
        Chrome::Windowed => {
            let inner = radius - ctx.state.chrome_metrics().border as f32;
            Corners::bottom(inner.max(0.0))
        }
        Chrome::Maximized => Corners::NONE,
        Chrome::None if flat => Corners::NONE,
        Chrome::None => Corners::all(radius),
    }
}

fn push_surface_tree(
    out: &mut Vec<DesiliumElement>,
    ctx: &SceneContext<'_>,
    tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    placement: &ClientPlacement,
    radii: Corners,
) {
    let program = ctx
        .resources
        .effects
        .as_ref()
        .and_then(|effects| effects.rounded_surface_program())
        .cloned();
    let clip = placement.clip;
    let motion = placement.motion;
    let Some(program) = program.filter(|_| radii.max() > 0.0) else {
        out.extend(tree.into_iter().map(|element| match motion {
            Some(motion) => DesiliumElement::from(move_element(element, motion)),
            None => DesiliumElement::from(element),
        }));
        return;
    };
    for element in tree {
        let clipped =
            RoundedSurfaceElement::new(element, program.clone(), clip.rect, radii, clip.scale);
        out.push(match motion {
            Some(motion) => DesiliumElement::from(move_element(NoOpaque(clipped), motion)),
            None => DesiliumElement::from(clipped),
        });
    }
}

fn move_element<E: Element>(
    element: E,
    motion: TreeMotion,
) -> RelocateRenderElement<RescaleRenderElement<E>> {
    let scaled = RescaleRenderElement::from_element(element, motion.pivot, motion.ratio);
    RelocateRenderElement::from_element(scaled, motion.offset, Relocate::Relative)
}
