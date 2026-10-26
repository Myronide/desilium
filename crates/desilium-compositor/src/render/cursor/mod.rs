//! Cursor rendering: themed bitmaps from `desilium-cursor` and client cursor surfaces.

mod clock;
mod layout;
mod themed;

use super::elements::DesiliumElement;
use super::scene::SceneContext;
use crate::state::CursorSource;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::input::pointer::CursorImageSurfaceData;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point};
use smithay::wayland::compositor::with_states;
use std::cell::RefCell;
use themed::ThemedCursors;

const OPAQUE: f32 = 1.0;

/// Cursor resources that outlive a frame: the loaded theme and uploaded cursor bitmaps.
pub struct Resources {
    themed: RefCell<ThemedCursors>,
}

impl Resources {
    pub fn new(_renderer: &mut GlesRenderer) -> Self {
        Self {
            themed: RefCell::new(ThemedCursors::new()),
        }
    }
}

/// Pushes the cursor element(s); called first, so the cursor is drawn above everything.
pub fn push(out: &mut Vec<DesiliumElement>, renderer: &mut GlesRenderer, ctx: &SceneContext<'_>) {
    let Some(pointer) = ctx.state.seat.get_pointer() else {
        return;
    };
    let location = pointer.current_location();
    let source = ctx.state.cursor_source();
    let themed = &ctx.resources.cursor.themed;
    match source {
        CursorSource::Hidden => themed.borrow_mut().hide(),
        CursorSource::Themed(kind) => {
            if let Some(element) = themed.borrow_mut().element(renderer, ctx, kind, location) {
                out.push(DesiliumElement::from(element));
            }
        }
        CursorSource::Surface(surface) => {
            themed.borrow_mut().hide();
            push_surface(out, renderer, ctx, &surface, location);
        }
    }
}

fn push_surface(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    surface: &WlSurface,
    location: Point<f64, Logical>,
) {
    let hotspot = surface_hotspot(surface);
    let origin: Point<i32, Physical> = (location - hotspot.to_f64())
        .to_physical(ctx.scale)
        .to_i32_round();
    let tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
        renderer,
        surface,
        origin,
        ctx.scale,
        OPAQUE,
        Kind::Cursor,
    );
    out.extend(tree.into_iter().map(DesiliumElement::from));
}

fn surface_hotspot(surface: &WlSurface) -> Point<i32, Logical> {
    with_states(surface, |states| {
        states
            .data_map
            .get::<CursorImageSurfaceData>()
            .and_then(|data| data.lock().ok().map(|attributes| attributes.hotspot))
    })
    .unwrap_or_default()
}
