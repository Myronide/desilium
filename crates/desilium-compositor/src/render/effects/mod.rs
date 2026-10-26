#![allow(unused_imports, dead_code)] // wired up in the integration step
//! GPU effect library: rounded rectangles, soft shadows, gradient backgrounds and
//! rounded clipping of client windows. Everything is a single-pass GLES2 shader.
//!
//! # Usage
//!
//! Compile once per GL context, keep one [`EffectSlot`] per visual, and ask for
//! the element every frame. The slot only touches the element's commit counter
//! when the parameters changed, so an idle desktop produces zero damage:
//!
//! ```ignore
//! let effects = Effects::compile(renderer)?;            // once
//! let mut slot = EffectSlot::default();                 // per window / panel
//! let shadow = Shadow::new(rect).corners(Corners::all(12.0)).scale(scale);
//! if let Some(element) = effects.shadow_in(&mut slot, &shadow) { /* push into the scene */ }
//! ```
//!
//! The one-shot `rounded_rect` / `shadow` / `gradient_background` functions create
//! a fresh element (and element id) on every call: fine for tests, wasteful for a
//! frame loop because a new id means full damage.
//!
//! # Rounded client windows
//!
//! Wrap the window's surface elements in [`RoundedSurfaceElement`] using
//! [`Effects::rounded_surface_program`]; see that type for the exact contract.
//! Draw order for one window (front to back): title bar `rounded_rect`, wrapped
//! client surfaces, border `rounded_rect` (transparent fill, `Outside` or `Inside`
//! border), `shadow` (with `knockout`).
//!
//! # Backdrop blur
//!
//! See the [`blur`] module: [`BackdropBlur`] is a render element with real frosted glass
//! (copy of what is below, dual-Kawase down-sample plus Gaussian, rounded composite) and
//! [`BlurSlot`] keeps its damage correct. Availability: [`EffectKind::BackdropBlur`].

mod blur;
mod layout;
mod surface;
mod types;

#[cfg(test)]
mod gpu_tests;

pub use blur::{BackdropBlur, BackdropBlurParams, BackdropWatch, BlurError, BlurSlot, SharedBlur};
pub use surface::RoundedSurfaceElement;
pub use types::{
    Border, BorderPlacement, Corners, Fill, GradientBackground, GradientStop, MAX_GRADIENT_STOPS,
    Rgba, RoundedRect, Shadow,
};

use layout::Prepared;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::{
    GlesError, GlesPixelProgram, GlesRenderer, GlesTexProgram, UniformName, UniformType,
};
use thiserror::Error;

const COMMON_SRC: &str = include_str!("../shaders/common.glsl");
const ROUNDED_RECT_SRC: &str = include_str!("../shaders/rounded_rect.frag");
const SHADOW_SRC: &str = include_str!("../shaders/shadow.frag");
const GRADIENT_SRC: &str = include_str!("../shaders/gradient.frag");
const ROUNDED_SURFACE_SRC: &str = include_str!("../shaders/rounded_surface.frag");

/// The individually compiled effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// Rounded rectangle with gradient and border.
    RoundedRect,
    /// Soft drop shadow.
    Shadow,
    /// Full-area gradient background.
    GradientBackground,
    /// Rounded clipping of client surfaces.
    RoundedSurface,
    /// Frosted-glass backdrop blur.
    BackdropBlur,
}

/// A shader that failed to compile or link.
#[derive(Debug, Error)]
#[error("effect {kind:?} failed to compile: {source}")]
pub struct EffectFailure {
    /// Which effect is unavailable.
    pub kind: EffectKind,
    /// The underlying GL error (the driver log is in the tracing output).
    #[source]
    pub source: GlesError,
}

/// Errors of [`Effects::compile`].
#[derive(Debug, Error)]
pub enum EffectsError {
    /// Not a single effect compiled; the GL context is probably unusable.
    #[error("no effect shader could be compiled ({} failures)", .0.len())]
    NothingCompiled(Vec<EffectFailure>),
}

/// Pixel-shader uniforms of each effect: (name, type).
type UniformSpec = &'static [(&'static str, UniformType)];

const ROUNDED_RECT_UNIFORMS: UniformSpec = &[
    ("u_phys", UniformType::_2f),
    ("u_rect", UniformType::_4f),
    ("u_radii", UniformType::_4f),
    ("u_color_a", UniformType::_4f),
    ("u_color_b", UniformType::_4f),
    ("u_grad", UniformType::_3f),
    ("u_border", UniformType::_4f),
    ("u_border_w", UniformType::_1f),
    ("u_border_outer", UniformType::_1f),
    ("u_alpha", UniformType::_1f),
    ("u_dither", UniformType::_1f),
];

const SHADOW_UNIFORMS: UniformSpec = &[
    ("u_phys", UniformType::_2f),
    ("u_rect", UniformType::_4f),
    ("u_radii", UniformType::_4f),
    ("u_sigma", UniformType::_1f),
    ("u_color", UniformType::_4f),
    ("u_knock", UniformType::_4f),
    ("u_knock_radii", UniformType::_4f),
    ("u_knock_on", UniformType::_1f),
    ("u_alpha", UniformType::_1f),
];

const GRADIENT_UNIFORMS: UniformSpec = &[
    ("u_phys", UniformType::_2f),
    ("u_grad", UniformType::_3f),
    ("u_c0", UniformType::_4f),
    ("u_c1", UniformType::_4f),
    ("u_c2", UniformType::_4f),
    ("u_c3", UniformType::_4f),
    ("u_pos", UniformType::_4f),
    ("u_alpha", UniformType::_1f),
    ("u_dither", UniformType::_1f),
];

const SURFACE_UNIFORMS: UniformSpec = &[
    ("u_tex_size", UniformType::_2f),
    ("u_src_loc", UniformType::_2f),
    ("u_src_size", UniformType::_2f),
    ("u_dst_size", UniformType::_2f),
    ("u_dst_offset", UniformType::_2f),
    ("u_clip_size", UniformType::_2f),
    ("u_radii", UniformType::_4f),
];

fn uniform_names(spec: UniformSpec) -> Vec<UniformName<'static>> {
    spec.iter()
        .map(|(name, kind)| UniformName::new(*name, *kind))
        .collect()
}

/// Compiled effect programs for one GL context.
#[derive(Debug)]
pub struct Effects {
    rounded_rect: Option<GlesPixelProgram>,
    shadow: Option<GlesPixelProgram>,
    gradient: Option<GlesPixelProgram>,
    surface: Option<GlesTexProgram>,
    backdrop_blur: Option<SharedBlur>,
    failures: Vec<EffectFailure>,
}

/// Keeps one visual's element and last parameters so unchanged frames add no damage.
#[derive(Debug)]
pub struct EffectSlot<P> {
    cached: Option<(P, PixelShaderElement)>,
}

impl<P> Default for EffectSlot<P> {
    fn default() -> Self {
        EffectSlot { cached: None }
    }
}

impl Effects {
    /// Compiles every effect. Individual failures are recorded (see [`Effects::failures`])
    /// and the affected accessor returns `None`, so callers can fall back to square corners;
    /// an error is returned only when nothing compiled at all.
    pub fn compile(renderer: &mut GlesRenderer) -> Result<Effects, EffectsError> {
        let mut failures = Vec::new();
        let mut pixel = |kind, src: &str, spec| {
            let source = format!("{COMMON_SRC}\n{src}");
            record(
                kind,
                renderer.compile_custom_pixel_shader(source, &uniform_names(spec)),
                &mut failures,
            )
        };
        let rounded_rect = pixel(
            EffectKind::RoundedRect,
            ROUNDED_RECT_SRC,
            ROUNDED_RECT_UNIFORMS,
        );
        let shadow = pixel(EffectKind::Shadow, SHADOW_SRC, SHADOW_UNIFORMS);
        let gradient = pixel(
            EffectKind::GradientBackground,
            GRADIENT_SRC,
            GRADIENT_UNIFORMS,
        );
        let surface = record(
            EffectKind::RoundedSurface,
            renderer.compile_custom_texture_shader(
                ROUNDED_SURFACE_SRC,
                &uniform_names(SURFACE_UNIFORMS),
            ),
            &mut failures,
        );
        let backdrop_blur = record(
            EffectKind::BackdropBlur,
            SharedBlur::compile(renderer).map_err(blur_failure),
            &mut failures,
        );
        let effects = Effects {
            rounded_rect,
            shadow,
            gradient,
            surface,
            backdrop_blur,
            failures,
        };
        if effects.available_count() == 0 {
            return Err(EffectsError::NothingCompiled(effects.failures));
        }
        Ok(effects)
    }

    /// Effects that failed to compile (empty on healthy drivers).
    pub fn failures(&self) -> &[EffectFailure] {
        &self.failures
    }

    /// Whether the given effect compiled and can be used.
    pub fn is_available(&self, kind: EffectKind) -> bool {
        match kind {
            EffectKind::RoundedRect => self.rounded_rect.is_some(),
            EffectKind::Shadow => self.shadow.is_some(),
            EffectKind::GradientBackground => self.gradient.is_some(),
            EffectKind::RoundedSurface => self.surface.is_some(),
            EffectKind::BackdropBlur => self.backdrop_blur.is_some(),
        }
    }

    fn available_count(&self) -> usize {
        [
            self.rounded_rect.is_some(),
            self.shadow.is_some(),
            self.gradient.is_some(),
            self.surface.is_some(),
            self.backdrop_blur.is_some(),
        ]
        .into_iter()
        .filter(|available| *available)
        .count()
    }

    /// Texture program that clips client content to a rounded rectangle; hand it to
    /// [`RoundedSurfaceElement::new`].
    pub fn rounded_surface_program(&self) -> Option<&GlesTexProgram> {
        self.surface.as_ref()
    }

    /// Shared GL resources of the backdrop blur; `None` when blur is unavailable and the
    /// caller must fall back to plain translucency. Hand it to [`BlurSlot::update`].
    pub fn backdrop_blur_resources(&self) -> Option<&SharedBlur> {
        self.backdrop_blur.as_ref()
    }

    /// Creates a fresh [`BackdropBlur`]. Prefer a [`BlurSlot`] in frame loops (stable id and
    /// correct damage); this is for tests and one-off renders.
    pub fn backdrop_blur(&self, params: BackdropBlurParams) -> Option<BackdropBlur> {
        Some(BackdropBlur::new(self.backdrop_blur.as_ref()?, params))
    }

    /// Creates a fresh rounded-rectangle element. Prefer [`Effects::rounded_rect_in`] in frame loops.
    pub fn rounded_rect(&self, params: &RoundedRect) -> Option<PixelShaderElement> {
        build(
            self.rounded_rect.as_ref()?,
            layout::prepare_rounded_rect(params)?,
        )
    }

    /// Creates a fresh shadow element. Prefer [`Effects::shadow_in`] in frame loops.
    pub fn shadow(&self, params: &Shadow) -> Option<PixelShaderElement> {
        build(self.shadow.as_ref()?, layout::prepare_shadow(params)?)
    }

    /// Creates a fresh gradient background element. Prefer [`Effects::gradient_background_in`] in frame loops.
    pub fn gradient_background(&self, params: &GradientBackground) -> Option<PixelShaderElement> {
        build(self.gradient.as_ref()?, layout::prepare_gradient(params)?)
    }

    /// Rounded rectangle with a stable element id; the commit counter moves only if `params` changed.
    pub fn rounded_rect_in(
        &self,
        slot: &mut EffectSlot<RoundedRect>,
        params: &RoundedRect,
    ) -> Option<PixelShaderElement> {
        let program = self.rounded_rect.as_ref()?;
        sync(slot, params, program, layout::prepare_rounded_rect)
    }

    /// Shadow with a stable element id; the commit counter moves only if `params` changed.
    pub fn shadow_in(
        &self,
        slot: &mut EffectSlot<Shadow>,
        params: &Shadow,
    ) -> Option<PixelShaderElement> {
        let program = self.shadow.as_ref()?;
        sync(slot, params, program, layout::prepare_shadow)
    }

    /// Gradient background with a stable element id; the commit counter moves only if `params` changed.
    pub fn gradient_background_in(
        &self,
        slot: &mut EffectSlot<GradientBackground>,
        params: &GradientBackground,
    ) -> Option<PixelShaderElement> {
        let program = self.gradient.as_ref()?;
        sync(slot, params, program, layout::prepare_gradient)
    }
}

fn blur_failure(error: BlurError) -> GlesError {
    tracing::warn!(%error, "backdrop blur pipeline unavailable");
    GlesError::ShaderCompileError
}

fn record<T>(
    kind: EffectKind,
    result: Result<T, GlesError>,
    failures: &mut Vec<EffectFailure>,
) -> Option<T> {
    match result {
        Ok(program) => Some(program),
        Err(source) => {
            tracing::warn!(?kind, error = %source, "effect shader unavailable, falling back");
            failures.push(EffectFailure { kind, source });
            None
        }
    }
}

fn build(program: &GlesPixelProgram, prepared: Prepared) -> Option<PixelShaderElement> {
    Some(PixelShaderElement::new(
        program.clone(),
        prepared.area,
        Some(prepared.opaque),
        1.0,
        prepared.uniforms,
        Kind::Unspecified,
    ))
}

fn sync<P: Clone + PartialEq>(
    slot: &mut EffectSlot<P>,
    params: &P,
    program: &GlesPixelProgram,
    prepare: fn(&P) -> Option<Prepared>,
) -> Option<PixelShaderElement> {
    if let Some((cached, element)) = &slot.cached
        && cached == params
    {
        return Some(element.clone());
    }
    let Some(prepared) = prepare(params) else {
        slot.cached = None;
        return None;
    };
    let Some((cached, element)) = slot.cached.as_mut() else {
        let element = build(program, prepared)?;
        slot.cached = Some((params.clone(), element.clone()));
        return Some(element);
    };
    element.resize(prepared.area, Some(prepared.opaque));
    element.update_uniforms(prepared.uniforms);
    *cached = params.clone();
    Some(element.clone())
}
