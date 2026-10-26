//! Rounded-corner clipping of client window content.
//!
//! Smithay 0.7 has no per-element texture program on `WaylandSurfaceRenderElement`.
//! The supported hook is `GlesFrame::override_default_tex_program`, which replaces
//! the program of every texture draw issued while it is active. [`RoundedSurfaceElement`]
//! wraps any `RenderElement<GlesRenderer>` (normally a `WaylandSurfaceRenderElement`),
//! activates the override around the inner `draw` and removes it again, so the
//! override can never leak onto a neighbouring element.

use super::types::Corners;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

const MAX_RADIUS: f32 = 4096.0;

/// Wraps a window render element and clips it to a rounded rectangle.
///
/// `clip` is the window geometry in the same logical space the element is
/// positioned in (output-global, i.e. what `Space::element_location` plus the
/// window geometry gives). Wrap every surface element of the toplevel (main
/// surface and its subsurfaces) with the same `clip`, but not popups: they may
/// extend beyond the window shape.
///
/// Limitations: only `Transform::Normal` buffers are clipped correctly, and the
/// texture size defaults to the element's source rectangle (exact unless the
/// client uses a `wp_viewporter` source crop; use [`Self::with_texture_size`] then).
#[derive(Debug)]
pub struct RoundedSurfaceElement<E> {
    inner: E,
    program: GlesTexProgram,
    clip: Rectangle<i32, Logical>,
    radii: Corners,
    scale: Scale<f64>,
    texture_size: Size<f64, Buffer>,
}

impl<E: Element> RoundedSurfaceElement<E> {
    /// Clips `inner` to `clip` with the given corner radii at output `scale`.
    pub fn new(
        inner: E,
        program: GlesTexProgram,
        clip: Rectangle<i32, Logical>,
        radii: Corners,
        scale: f64,
    ) -> Self {
        let src = inner.src();
        let texture_size = Size::from((src.loc.x + src.size.w, src.loc.y + src.size.h));
        RoundedSurfaceElement {
            inner,
            program,
            clip,
            radii,
            scale: Scale::from(scale),
            texture_size,
        }
    }

    /// Overrides the assumed buffer texture size (needed for cropped viewport sources).
    pub fn with_texture_size(mut self, size: Size<i32, Buffer>) -> Self {
        self.texture_size = size.to_f64();
        self
    }

    /// The wrapped element.
    pub fn inner(&self) -> &E {
        &self.inner
    }

    fn clip_px(&self) -> Rectangle<i32, Physical> {
        self.clip.to_physical_precise_round(self.scale)
    }

    fn radii_px(&self) -> [f32; 4] {
        let scale = self.scale.x as f32;
        self.radii
            .sanitized()
            .map(|radius| radius.min(MAX_RADIUS) * scale)
    }

    fn corner_squares(&self, origin: Point<i32, Physical>) -> Vec<Rectangle<i32, Physical>> {
        let clip = self.clip_px();
        let [top_left, top_right, bottom_right, bottom_left] =
            self.radii_px().map(|radius| radius.ceil() as i32);
        let loc = clip.loc - origin;
        let (width, height) = (clip.size.w, clip.size.h);
        vec![
            Rectangle::new(loc, (top_left, top_left).into()),
            Rectangle::new(
                (loc.x + width - top_right, loc.y).into(),
                (top_right, top_right).into(),
            ),
            Rectangle::new(
                (loc.x + width - bottom_right, loc.y + height - bottom_right).into(),
                (bottom_right, bottom_right).into(),
            ),
            Rectangle::new(
                (loc.x, loc.y + height - bottom_left).into(),
                (bottom_left, bottom_left).into(),
            ),
        ]
    }

    fn uniforms(
        &self,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
    ) -> Vec<Uniform<'static>> {
        let clip = self.clip_px();
        let offset = dst.loc - clip.loc;
        vec![
            Uniform::new(
                "u_tex_size",
                (self.texture_size.w as f32, self.texture_size.h as f32),
            ),
            Uniform::new("u_src_loc", (src.loc.x as f32, src.loc.y as f32)),
            Uniform::new("u_src_size", (src.size.w as f32, src.size.h as f32)),
            Uniform::new("u_dst_size", (dst.size.w as f32, dst.size.h as f32)),
            Uniform::new("u_dst_offset", (offset.x as f32, offset.y as f32)),
            Uniform::new("u_clip_size", (clip.size.w as f32, clip.size.h as f32)),
            Uniform::new("u_radii", self.radii_px()),
        ]
    }
}

impl<E: Element> Element for RoundedSurfaceElement<E> {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner.damage_since(scale, commit)
    }

    // The corner pixels are translucent, so the inner opaque regions must not cover them:
    // otherwise the damage tracker would skip what lies below or disable blending there.
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        let inner = self.inner.opaque_regions(scale);
        let squares = self.corner_squares(self.inner.geometry(scale).loc);
        inner
            .into_iter()
            .flat_map(|region| region.subtract_rects(squares.iter().copied()))
            .collect()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl<E: RenderElement<GlesRenderer>> RenderElement<GlesRenderer> for RoundedSurfaceElement<E> {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        frame.override_default_tex_program(self.program.clone(), self.uniforms(src, dst));
        let result = self.inner.draw(frame, src, dst, damage, opaque_regions);
        frame.clear_tex_program_override();
        result
    }

    // Direct scanout would bypass the shader and show square corners.
    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
