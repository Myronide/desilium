//! Styled xdg popups (context menus, dropdowns, tooltips).
//!
//! Clients draw popups as plain rectangles. The compositor gives them the same finish as windows:
//! a rounded clip, a hairline border, the theme's popup shadow and, when the client surface has
//! transparent pixels, frosted glass behind it. Opaque menus (most GTK and Qt ones) skip the blur,
//! because it would be hidden entirely.

use super::SceneContext;
use super::effects::{
    BackdropBlurParams, Border, Corners, EffectSlot, RoundedRect, RoundedSurfaceElement, Shadow,
};
use super::elements::DesiliumElement;
use super::glass::GlassKey;
use super::scene::rgba;
use desilium_theme::{Color, Theme};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::{Element, Kind};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::utils::bbox_from_surface_tree;
use smithay::desktop::{PopupKind, PopupManager};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size};
use std::cell::RefCell;
use std::collections::HashMap;

const OPAQUE: f32 = 1.0;
/// Popups are a little rounder than controls so the panel reads as a floating layer above them.
const RADIUS_FACTOR: f32 = 1.25;
const BORDER_WIDTH: f32 = 1.0;
/// Share of the text colour mixed into the panel border: a faint light edge on dark, dark on light.
const BORDER_STRENGTH: f32 = 0.14;

/// Cached shader elements of every popup on screen, keyed by its surface.
#[derive(Default)]
pub struct Resources {
    inner: RefCell<Inner>,
}

#[derive(Default)]
struct Inner {
    frame: u64,
    entries: HashMap<ObjectId, Entry>,
}

#[derive(Default)]
struct Entry {
    shadow: EffectSlot<Shadow>,
    border: EffectSlot<RoundedRect>,
    used: u64,
}

impl Resources {
    /// Starts a scene build.
    pub fn begin_frame(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.frame = inner.frame.wrapping_add(1);
        }
    }

    /// Drops the slots of popups that are gone.
    pub fn end_frame(&self) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let frame = inner.frame;
        inner.entries.retain(|_, entry| entry.used == frame);
    }
}

/// Corner radius of a popup of `size`, never more than half its shorter side.
pub fn popup_radius(theme: &Theme, size: Size<i32, Logical>) -> f32 {
    let wanted = theme.window.control_radius * RADIUS_FACTOR;
    let half_short_side = size.w.min(size.h).max(0) as f32 / 2.0;
    wanted.min(half_short_side).max(0.0)
}

/// The hairline around a popup: the text colour at low strength, so it works in both modes.
pub fn border_color(theme: &Theme) -> Color {
    theme.palette.text_primary.with_alpha(BORDER_STRENGTH)
}

/// The theme's popup shadow around `rect`.
pub fn shadow_params(
    theme: &Theme,
    rect: Rectangle<i32, Logical>,
    scale: f64,
    radius: f32,
) -> Shadow {
    let style = &theme.shadow.popup;
    Shadow::new(rect)
        .scale(scale)
        .corners(Corners::all(radius))
        .blur(style.blur, style.spread)
        .offset(style.offset_x, style.offset_y)
        .color(rgba(style.color))
}

/// True when opaque `regions` (absolute coordinates) cover all of `area`.
fn covers(regions: &[Rectangle<i32, Physical>], area: Rectangle<i32, Physical>) -> bool {
    regions.iter().any(|region| region.contains_rect(area))
}

/// Whether the client surface has see-through pixels inside `area`, so glass behind it shows.
fn is_translucent(
    tree: &[WaylandSurfaceRenderElement<GlesRenderer>],
    area: Rectangle<i32, Physical>,
    scale: Scale<f64>,
) -> bool {
    let regions: Vec<Rectangle<i32, Physical>> = tree
        .iter()
        .flat_map(|element| {
            let origin = element.geometry(scale).loc;
            element
                .opaque_regions(scale)
                .into_iter()
                .map(move |region| Rectangle::new(region.loc + origin, region.size))
        })
        .collect();
    !covers(&regions, area)
}

/// Where one popup sits.
struct Spot {
    /// The popup's window geometry in output coordinates.
    rect: Rectangle<i32, Logical>,
    /// Origin of its surface tree in device pixels.
    origin: Point<i32, Physical>,
}

/// The popup's window geometry; clients that never set one get the bounds of their surface tree.
fn geometry_of(popup: &PopupKind) -> Rectangle<i32, Logical> {
    let declared = popup.geometry();
    if !declared.is_empty() {
        return declared;
    }
    bbox_from_surface_tree(popup.wl_surface(), (0, 0))
}

fn spot(
    popup: &PopupKind,
    parent_origin: Point<i32, Logical>,
    offset: Point<i32, Logical>,
    scale: f64,
) -> Spot {
    let geometry = geometry_of(popup);
    let top_left = parent_origin + offset;
    Spot {
        rect: Rectangle::new(top_left, geometry.size),
        origin: (top_left - geometry.loc).to_physical_precise_round(scale),
    }
}

/// Draws the popups of `parent` (a toplevel's or layer surface's root), deepest submenu on top.
///
/// `parent_origin` is where the parent's popup coordinates start in output space.
pub fn push(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    parent_origin: Point<i32, Logical>,
    parent: &WlSurface,
) {
    let popups: Vec<(PopupKind, Point<i32, Logical>)> =
        PopupManager::popups_for_surface(parent).collect();
    for (popup, offset) in popups.iter().rev() {
        let at = spot(popup, parent_origin, *offset, ctx.scale);
        push_one(out, renderer, ctx, popup.wl_surface(), &at);
    }
}

fn push_one(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    surface: &WlSurface,
    at: &Spot,
) {
    let tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
        renderer,
        surface,
        at.origin,
        ctx.scale,
        OPAQUE,
        Kind::Unspecified,
    );
    let theme = ctx.state.theme();
    let (Some(effects), false) = (ctx.resources.effects.as_ref(), at.rect.is_empty()) else {
        out.extend(tree.into_iter().map(DesiliumElement::from));
        return;
    };
    let radius = popup_radius(theme, at.rect.size);
    let scale = Scale::from(ctx.scale);
    let translucent = is_translucent(&tree, at.rect.to_physical_precise_round(scale), scale);
    let Ok(mut inner) = ctx.resources.popups.inner.try_borrow_mut() else {
        out.extend(tree.into_iter().map(DesiliumElement::from));
        return;
    };
    let frame = inner.frame;
    let entry = inner.entries.entry(surface.id()).or_default();
    entry.used = frame;

    let border = RoundedRect::new(at.rect, super::effects::Rgba::TRANSPARENT)
        .scale(ctx.scale)
        .radius(radius)
        .border(Border::inside(BORDER_WIDTH, rgba(border_color(theme))));
    out.extend(
        effects
            .rounded_rect_in(&mut entry.border, &border)
            .map(DesiliumElement::from),
    );
    push_clipped(out, ctx, tree, at.rect, radius);
    if translucent && theme.effects.blur_radius > 0.0 {
        let glass = BackdropBlurParams::new(at.rect, theme.effects.blur_radius)
            .scale(ctx.scale)
            .corners(Corners::all(radius))
            .saturation(theme.effects.backdrop_saturation);
        ctx.resources
            .glass
            .request(GlassKey::surface(surface), out.len(), glass);
    }
    let shadow = shadow_params(theme, at.rect, ctx.scale, radius);
    out.extend(
        effects
            .shadow_in(&mut entry.shadow, &shadow)
            .map(DesiliumElement::from),
    );
}

/// Clips every element of the popup's tree to its rounded geometry (plain if no program).
fn push_clipped(
    out: &mut Vec<DesiliumElement>,
    ctx: &SceneContext<'_>,
    tree: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    clip: Rectangle<i32, Logical>,
    radius: f32,
) {
    let program = ctx
        .resources
        .effects
        .as_ref()
        .and_then(|effects| effects.rounded_surface_program())
        .cloned();
    let Some(program) = program else {
        out.extend(tree.into_iter().map(DesiliumElement::from));
        return;
    };
    for element in tree {
        let clipped = RoundedSurfaceElement::new(
            element,
            program.clone(),
            clip,
            Corners::all(radius),
            ctx.scale,
        );
        out.push(DesiliumElement::from(clipped));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn radius_follows_the_control_radius_and_shrinks_for_tiny_popups() {
        let theme = Theme::dark();
        let big = popup_radius(&theme, (200, 150).into());
        assert!((big - theme.window.control_radius * RADIUS_FACTOR).abs() < 1e-6);
        assert_eq!(popup_radius(&theme, (200, 6).into()), 3.0);
        assert_eq!(popup_radius(&theme, (0, 0).into()), 0.0);
    }

    #[test]
    fn one_opaque_region_over_the_whole_popup_means_no_glass() {
        let whole = [area(0, 0, 100, 80)];
        assert!(covers(&whole, area(0, 0, 100, 80)));
        assert!(covers(&whole, area(10, 10, 50, 50)));
    }

    #[test]
    fn partial_or_missing_opaque_regions_mean_glass() {
        assert!(!covers(&[], area(0, 0, 100, 80)));
        assert!(!covers(&[area(0, 0, 100, 40)], area(0, 0, 100, 80)));
        assert!(!covers(&[area(0, 0, 99, 80)], area(0, 0, 100, 80)));
    }

    #[test]
    fn border_is_faint_in_both_modes() {
        for theme in [Theme::dark(), Theme::light()] {
            let border = border_color(&theme);
            assert!((border.a - BORDER_STRENGTH).abs() < 1e-6);
        }
    }

    #[test]
    fn shadow_uses_the_theme_popup_elevation() {
        let theme = Theme::dark();
        let rect = Rectangle::new((10, 10).into(), (100, 100).into());
        let shadow = shadow_params(&theme, rect, 2.0, 10.0);
        assert_eq!(shadow.blur, theme.shadow.popup.blur);
        assert_eq!(shadow.offset_y, theme.shadow.popup.offset_y);
        assert_eq!(shadow.scale, 2.0);
        assert!(shadow.knockout);
    }
}
