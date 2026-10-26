//! Render side of the server-side window decorations: shadow, border, title bar, title text and
//! window buttons.
//!
//! The look is described by [`plan::plan`]; this module turns it into GPU elements and keeps every
//! texture and shader element cached so an unchanged window adds no damage.

mod glyphs;
mod plan;
mod title;

use super::SceneContext;
use super::effects::{
    BackdropBlurParams, Border, Corners, EffectSlot, Effects, Fill, Rgba, RoundedRect, Shadow,
};
use super::elements::DesiliumElement;
use super::glass::{self, GlassKey};
use super::scene::rgba;
use crate::animation::Xform;
use crate::decoration::{Button, GlyphKind, Metrics};
use crate::state::SceneWindow;
use desilium_text::{FontStack, TextRenderer};
use desilium_theme::{Color, Theme};
use desilium_wm::{Rect, WindowId};
use glyphs::GlyphKey;
use plan::{Look, Paint};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Physical, Point, Rectangle};
use std::cell::RefCell;
use std::collections::HashMap;
use title::{TitleImage, TitleKey};
use tracing::warn;

/// Where the visual centre of lower-case title text sits relative to the baseline, in em.
/// Cap height is about 0.72 em, so the optical centre is 0.36 em above the baseline.
const CAP_CENTRE_EM: f32 = 0.36;

/// Which drawing of a window an effect slot belongs to; snapshots must not disturb the slots
/// (and therefore the damage) of what is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Screen,
    Snapshot,
}

/// How one window is drawn this time: its transform and the cache layer for its effect slots.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub xform: Xform,
    pub layer: Layer,
}

/// Which rectangle of a window an effect slot belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Part {
    Border,
    TitleBar,
    Fill(Button),
}

struct Cached<T> {
    value: T,
    used: u64,
}

#[derive(Default)]
struct Inner {
    text: Option<TextRenderer>,
    titles: HashMap<TitleKey, Cached<Option<TitleImage>>>,
    glyphs: HashMap<GlyphKey, Cached<Option<MemoryRenderBuffer>>>,
    rects: HashMap<(WindowId, Part, Layer), Cached<EffectSlot<RoundedRect>>>,
    shadows: HashMap<WindowId, Cached<EffectSlot<Shadow>>>,
    frame: u64,
}

/// GL-bound decoration resources (text renderer, glyph and title textures, stable effect slots).
///
/// Interior mutability because the scene is built from a shared [`SceneContext`]; rendering is
/// single threaded.
pub struct Resources {
    inner: RefCell<Inner>,
}

impl Resources {
    pub fn new(_renderer: &mut GlesRenderer) -> Self {
        let text = match TextRenderer::new(FontStack::sans()) {
            Ok(text) => Some(text),
            Err(error) => {
                warn!(%error, "no usable font; window titles are not drawn");
                None
            }
        };
        Self {
            inner: RefCell::new(Inner {
                text,
                ..Inner::default()
            }),
        }
    }

    /// Starts a scene build.
    pub fn begin_frame(&self) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.frame = inner.frame.wrapping_add(1);
        }
    }

    /// Drops everything the finished scene did not use (closed windows, old titles).
    pub fn end_frame(&self) {
        let Ok(mut inner) = self.inner.try_borrow_mut() else {
            return;
        };
        let frame = inner.frame;
        inner.titles.retain(|_, entry| entry.used == frame);
        inner.glyphs.retain(|_, entry| entry.used == frame);
        inner.rects.retain(|_, entry| entry.used == frame);
        inner.shadows.retain(|_, entry| entry.used == frame);
    }
}

fn logical_rect(rect: Rect) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (rect.loc.x, rect.loc.y).into(),
        (rect.size.w, rect.size.h).into(),
    )
}

/// Draws border, buttons, title text and title bar of `window`, topmost first.
pub fn push_chrome(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    window: &SceneWindow,
    view: View,
) {
    let theme = ctx.state.theme();
    let metrics = ctx.state.chrome_metrics();
    let translucency =
        glass::debug::titlebar_translucency(ctx).unwrap_or(theme.effects.titlebar_translucency);
    let look = Look {
        focus: window.focus,
        maximized: window.state == desilium_wm::WindowState::Maximized,
        fades: window.fades,
        bar_opacity: plan::title_opacity(translucency),
    };
    let paints = plan::plan(theme, &metrics, &window.frame, &look);
    let Ok(mut inner) = ctx.resources.decoration.try_inner() else {
        return;
    };
    let mut pass = Pass {
        inner: &mut inner,
        renderer,
        effects: ctx.resources.effects.as_ref(),
        glass: &ctx.resources.glass,
        scale: ctx.scale,
        window,
        view,
        theme,
        metrics: &metrics,
        glass_behind_bar: view.layer == Layer::Screen
            && view.xform.is_identity()
            && look.bar_opacity < 1.0,
        out,
    };
    for paint in paints {
        pass.paint(paint);
    }
}

/// What a window's drop shadow needs to know.
#[derive(Debug, Clone, Copy)]
pub struct ShadowCaster {
    pub id: WindowId,
    /// The window's final layout rectangle.
    pub caster: Rect,
    pub radius: f32,
    /// Active/inactive blend, see [`SceneWindow::focus`].
    pub focus: f32,
    pub xform: Xform,
}

/// Draws the drop shadow around a window's frame (or its client when undecorated).
///
/// `caster` is the window's final layout; the window's animation transform moves, scales and
/// fades the shadow together with the rest of the window.
pub fn push_shadow(out: &mut Vec<DesiliumElement>, ctx: &SceneContext<'_>, shadow: ShadowCaster) {
    let Some(effects) = ctx.resources.effects.as_ref() else {
        return;
    };
    let Ok(mut inner) = ctx.resources.decoration.try_inner() else {
        return;
    };
    let xform = shadow.xform;
    let unit = xform.uniform();
    let style = plan::shadow_style(ctx.state.theme(), shadow.focus);
    let shadow_params = Shadow::new(logical_rect(xform.rect(shadow.caster)))
        .scale(ctx.scale)
        .corners(Corners::all(shadow.radius * unit))
        .blur(style.blur * unit, style.spread * unit)
        .offset(style.offset_x * unit, style.offset_y * unit)
        .color(rgba(style.color))
        .alpha(xform.alpha);
    let frame = inner.frame;
    let entry = inner.shadows.entry(shadow.id).or_insert_with(|| Cached {
        value: EffectSlot::default(),
        used: frame,
    });
    entry.used = frame;
    out.extend(
        effects
            .shadow_in(&mut entry.value, &shadow_params)
            .map(DesiliumElement::from),
    );
}

impl Resources {
    fn try_inner(&self) -> Result<std::cell::RefMut<'_, Inner>, std::cell::BorrowMutError> {
        self.inner.try_borrow_mut()
    }
}

struct Pass<'a> {
    inner: &'a mut Inner,
    renderer: &'a mut GlesRenderer,
    effects: Option<&'a Effects>,
    glass: &'a glass::Glass,
    scale: f64,
    window: &'a SceneWindow,
    view: View,
    theme: &'a Theme,
    metrics: &'a Metrics,
    /// Whether the title bar gets real backdrop blur: only at rest and on screen (a moving window
    /// has no stable backdrop, and snapshots are rendered into their own list).
    glass_behind_bar: bool,
    out: &'a mut Vec<DesiliumElement>,
}

impl Pass<'_> {
    fn paint(&mut self, paint: Paint) {
        match paint {
            Paint::Border {
                rect,
                radius,
                width,
                color,
            } => {
                let unit = self.view.xform.uniform();
                let width = (width as f32 * unit).round().max(1.0);
                let border = Border::inside(width, rgba(color));
                let params = self
                    .rect(rect, Rgba::TRANSPARENT)
                    .radius(radius * unit)
                    .border(border);
                self.push_rect(Part::Border, params);
            }
            Paint::Glyph { kind, rect, color } => self.glyph(kind, rect, color),
            Paint::ButtonFill {
                button,
                rect,
                radius,
                color,
            } => {
                let radius = radius * self.view.xform.uniform();
                let params = self.rect(rect, rgba(color)).radius(radius);
                self.push_rect(Part::Fill(button), params);
            }
            Paint::Title { area, color } => self.title(area, color),
            Paint::TitleBar {
                rect,
                top_radius,
                top,
                bottom,
            } => {
                let fill = Fill::vertical(rgba(top), rgba(bottom));
                let top_radius = top_radius * self.view.xform.uniform();
                let params = self
                    .rect(rect, rgba(bottom))
                    .corners(Corners::top(top_radius))
                    .fill(fill);
                self.push_rect(Part::TitleBar, params);
                self.request_bar_glass(rect, top_radius);
            }
        }
    }

    /// Records the glass right below the title bar fill that was just pushed.
    fn request_bar_glass(&self, rect: Rect, top_radius: f32) {
        if !self.glass_behind_bar || self.theme.effects.blur_radius <= 0.0 {
            return;
        }
        let params = BackdropBlurParams::new(logical_rect(rect), self.theme.effects.blur_radius)
            .scale(self.scale)
            .corners(Corners::top(top_radius))
            .saturation(self.theme.effects.backdrop_saturation);
        self.glass
            .request(GlassKey::TitleBar(self.window.id), self.out.len(), params);
    }

    fn rect(&self, rect: Rect, color: Rgba) -> RoundedRect {
        RoundedRect::new(logical_rect(self.view.xform.rect(rect)), color)
            .scale(self.scale)
            .alpha(self.view.xform.alpha)
    }

    fn push_rect(&mut self, part: Part, params: RoundedRect) {
        let Some(effects) = self.effects else {
            return;
        };
        let frame = self.inner.frame;
        let entry = self
            .inner
            .rects
            .entry((self.window.id, part, self.view.layer))
            .or_insert_with(|| Cached {
                value: EffectSlot::default(),
                used: frame,
            });
        entry.used = frame;
        self.out.extend(
            effects
                .rounded_rect_in(&mut entry.value, &params)
                .map(DesiliumElement::from),
        );
    }

    fn glyph(&mut self, kind: GlyphKind, rect: Rect, color: Color) {
        let edge_px = device_len(rect.size.w, self.scale);
        let key = GlyphKey {
            kind,
            edge_px,
            stroke_bits: (self.metrics.glyph_stroke * self.scale as f32).to_bits(),
            color: glyphs::bytes(color),
        };
        let frame = self.inner.frame;
        let entry = self.inner.glyphs.entry(key).or_insert_with(|| Cached {
            value: glyphs::render(&key),
            used: frame,
        });
        entry.used = frame;
        let Some(buffer) = entry.value.as_ref() else {
            return;
        };
        let moved = self.view.xform.rect(rect);
        let location = device_point(moved.loc.x, moved.loc.y, self.scale);
        let size = (moved.size.w, moved.size.h);
        let pixels = (edge_px as i32, edge_px as i32);
        let alpha = self.view.xform.alpha;
        push_memory(
            self.out,
            self.renderer,
            buffer,
            location,
            size,
            pixels,
            alpha,
        );
    }

    fn title(&mut self, area: Rect, color: Color) {
        if area.size.w <= 0 || self.window.title.is_empty() {
            return;
        }
        let window = &self.theme.window;
        let key = TitleKey {
            text: self.window.title.clone(),
            width_px: device_len(area.size.w, self.scale),
            scale_bits: (self.scale as f32).to_bits(),
            size_bits: window.title_font_size.to_bits(),
            color: glyphs::bytes(color),
            weight: window.title_font_weight,
        };
        let frame = self.inner.frame;
        let Inner { text, titles, .. } = &mut *self.inner;
        let Some(text) = text.as_mut() else {
            return;
        };
        let entry = titles.entry(key.clone()).or_insert_with(|| Cached {
            value: title::render(text, &key),
            used: frame,
        });
        entry.used = frame;
        let Some(image) = entry.value.as_ref() else {
            return;
        };
        let centre = f64::from(area.loc.y) + f64::from(area.size.h) / 2.0;
        let baseline = centre + f64::from(CAP_CENTRE_EM * window.title_font_size);
        let (left, baseline) = self.view.xform.point(f64::from(area.loc.x), baseline);
        let (scale_x, scale_y) = (self.view.xform.sx, self.view.xform.sy);
        let location: Point<f64, Physical> = (
            (left * self.scale).round(),
            (baseline * self.scale).round() - f64::from(image.baseline) * scale_y,
        )
            .into();
        let size = (
            scaled_len(image.logical.0, scale_x),
            scaled_len(image.logical.1, scale_y),
        );
        let alpha = self.view.xform.alpha;
        push_memory(
            self.out,
            self.renderer,
            &image.buffer,
            location,
            size,
            image.pixels,
            alpha,
        );
    }
}

fn push_memory(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    buffer: &MemoryRenderBuffer,
    location: Point<f64, Physical>,
    logical: (i32, i32),
    pixels: (i32, i32),
    alpha: f32,
) {
    // The source covers the whole bitmap (buffer scale 1) while the destination is the logical
    // size, so the texture is mapped 1:1 onto device pixels.
    let source =
        Rectangle::<f64, Logical>::from_size((f64::from(pixels.0), f64::from(pixels.1)).into());
    let element = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        location,
        buffer,
        Some(alpha),
        Some(source),
        Some(logical.into()),
        Kind::Unspecified,
    );
    match element {
        Ok(element) => out.push(DesiliumElement::from(element)),
        Err(error) => warn!(%error, "cannot upload decoration texture"),
    }
}

fn scaled_len(length: i32, factor: f64) -> i32 {
    (f64::from(length) * factor).round().max(1.0) as i32
}

fn device_len(logical: i32, scale: f64) -> u32 {
    (f64::from(logical.max(1)) * scale).round().max(1.0) as u32
}

fn device_point(x: i32, y: i32, scale: f64) -> Point<f64, Physical> {
    (
        (f64::from(x) * scale).round(),
        (f64::from(y) * scale).round(),
    )
        .into()
}
