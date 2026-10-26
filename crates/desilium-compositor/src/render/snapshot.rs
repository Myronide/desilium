//! Window snapshots: the client surface is gone the moment a window is destroyed, so the close
//! animation fades a picture of the window's last frame instead.
//!
//! A snapshot is taken while a close is likely or requested (see `state::animation`), by
//! rendering the window (frame and client, without shadow) into an offscreen texture.

use super::SceneContext;
use super::decoration::{self, Layer, ShadowCaster, View};
use super::elements::DesiliumElement;
use super::scene::push_window;
use crate::animation::{RectF, Xform};
use crate::state::{ClosingWindow, SceneWindow};
use desilium_theme::Theme;
use desilium_wm::{Rect, WindowId};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Size, Transform};
use std::cell::RefCell;
use std::collections::HashMap;
use tracing::warn;

const TRANSPARENT: [f32; 4] = [0.0; 4];

struct Snapshot {
    buffer: TextureBuffer<GlesTexture>,
    /// Pixel size of the texture.
    pixels: Size<i32, Physical>,
    /// The frame rectangle the snapshot shows, in its final layout.
    layout: Rect,
}

/// The newest snapshot of every window that asked for one.
#[derive(Default)]
pub struct Snapshots {
    entries: RefCell<HashMap<WindowId, Snapshot>>,
}

impl Snapshots {
    /// Renders `window` into a fresh texture; returns whether a snapshot now exists.
    pub fn capture(
        &self,
        renderer: &mut GlesRenderer,
        ctx: &SceneContext<'_>,
        theme: &Theme,
        window: &SceneWindow,
    ) -> bool {
        match render_snapshot(renderer, ctx, theme, window) {
            Ok(snapshot) => {
                self.entries.borrow_mut().insert(window.id, snapshot);
                true
            }
            Err(error) => {
                warn!(%error, "cannot snapshot window");
                self.entries.borrow_mut().remove(&window.id);
                false
            }
        }
    }

    /// Drops snapshots of windows `keep` rejects and returns their ids.
    pub fn retain(&self, mut keep: impl FnMut(WindowId) -> bool) -> Vec<WindowId> {
        let mut dropped = Vec::new();
        self.entries.borrow_mut().retain(|id, _| {
            let kept = keep(*id);
            if !kept {
                dropped.push(*id);
            }
            kept
        });
        dropped
    }

    fn element(
        &self,
        ctx: &SceneContext<'_>,
        ghost: &ClosingWindow,
    ) -> Option<TextureRenderElement<GlesTexture>> {
        let entries = self.entries.borrow();
        let snapshot = entries.get(&ghost.id)?;
        let moved = ghost.xform.rect(snapshot.layout);
        let location: Point<f64, Physical> = (
            (f64::from(moved.loc.x) * ctx.scale).round(),
            (f64::from(moved.loc.y) * ctx.scale).round(),
        )
            .into();
        let source = Rectangle::<f64, Logical>::from_size(
            (f64::from(snapshot.pixels.w), f64::from(snapshot.pixels.h)).into(),
        );
        let size: Size<i32, Logical> = (moved.size.w, moved.size.h).into();
        Some(TextureRenderElement::from_texture_buffer(
            location,
            &snapshot.buffer,
            Some(ghost.xform.alpha),
            Some(source),
            Some(size),
            Kind::Unspecified,
        ))
    }
}

/// Draws a closing window: its shadow and its snapshot, both faded and scaled by its pose.
pub fn push_ghost(out: &mut Vec<DesiliumElement>, ctx: &SceneContext<'_>, ghost: &ClosingWindow) {
    let Some(element) = ctx.resources.snapshots.element(ctx, ghost) else {
        return;
    };
    out.push(DesiliumElement::from(element));
    let shadow = ShadowCaster {
        id: ghost.id,
        caster: ghost.layout,
        radius: ctx.state.theme().window.corner_radius,
        focus: ghost.focus,
        xform: ghost.xform,
    };
    decoration::push_shadow(out, ctx, shadow);
}

fn render_snapshot(
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    theme: &Theme,
    window: &SceneWindow,
) -> Result<Snapshot, String> {
    let layout = window.frame.outer;
    let pixels: Size<i32, Physical> = (
        device_len(layout.size.w, ctx.scale),
        device_len(layout.size.h, ctx.scale),
    )
        .into();
    // Draw the window with its frame's corner at the texture origin and nothing else.
    let target_rect = RectF {
        x: 0.0,
        y: 0.0,
        w: layout.size.w as f32,
        h: layout.size.h as f32,
    };
    let view = View {
        xform: Xform::mapping(RectF::from_rect(layout), target_rect),
        layer: Layer::Snapshot,
    };
    let mut elements = Vec::new();
    push_window(&mut elements, renderer, ctx, theme, window, view);

    let buffer_size: Size<i32, Buffer> = (pixels.w, pixels.h).into();
    let mut texture =
        Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size)
            .map_err(|error| error.to_string())?;
    {
        let mut target = renderer
            .bind(&mut texture)
            .map_err(|error| error.to_string())?;
        let mut tracker = OutputDamageTracker::new(pixels, ctx.scale, Transform::Normal);
        tracker
            .render_output(renderer, &mut target, 0, &elements, TRANSPARENT)
            .map_err(|error| error.to_string())?;
    }
    let buffer = TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
    Ok(Snapshot {
        buffer,
        pixels,
        layout,
    })
}

fn device_len(logical: i32, scale: f64) -> i32 {
    (f64::from(logical) * scale).round().max(1.0) as i32
}
