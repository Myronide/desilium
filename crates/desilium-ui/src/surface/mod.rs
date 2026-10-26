//! [`UiSurface`]: one HTML/CSS document rendered to a pixel buffer, with input and actions.
//!
//! The surface owns the Blitz document and the CPU rasteriser; nothing Blitz
//! leaks through its public API. Single-threaded: create it, drive it and drop
//! it on one thread (the rasteriser may use worker threads internally when the
//! `multithreading` feature is on).

mod document;
mod dom;
mod input;
mod render;
mod shell;

use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Instant};

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::NodeId;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use desilium_theme::{Mode, Theme};

pub use dom::Updates;
pub use render::bgra_premultiplied_to_rgba;

use crate::{
    UiError,
    input::{PixelRect, UiEvent},
};
use shell::SharedState;

/// Largest buffer edge in physical pixels (the rasteriser addresses pixels with `u16`).
const MAX_EDGE: u32 = 16_384;
const MIN_SCALE: f64 = 0.25;
const MAX_SCALE: f64 = 8.0;
const BYTES_PER_PIXEL: usize = 4;

/// How to create a [`UiSurface`].
#[derive(Debug, Clone)]
pub struct SurfaceOptions {
    /// Logical width in CSS pixels.
    pub width: u32,
    /// Logical height in CSS pixels.
    pub height: u32,
    /// Device scale factor (may be fractional).
    pub scale: f64,
    /// Theme whose tokens are applied to the document root.
    pub theme: Theme,
    /// Bundled font files (TTF/OTF/TTC) registered before the document is built,
    /// so `Inter` resolves identically on every machine.
    pub fonts: Vec<Vec<u8>>,
    /// Directory relative resource URLs (`<img src>`) resolve against.
    pub base_dir: Option<PathBuf>,
}

impl SurfaceOptions {
    /// Dark theme, scale 1, system fonts only.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale: 1.0,
            theme: Theme::dark(),
            fonts: Vec::new(),
            base_dir: None,
        }
    }
}

/// An HTML/CSS surface that renders to a caller-provided `Argb8888` buffer.
pub struct UiSurface {
    doc: HtmlDocument,
    renderer: VelloCpuImageRenderer,
    shared: Arc<SharedState>,
    theme: Theme,
    width: u32,
    height: u32,
    scale: f64,
    epoch: Instant,
    dirty: bool,
    frame: render::FrameBuffers,
    pointer: input::PointerState,
    events: VecDeque<UiEvent>,
    focus_before_leave: Option<NodeId>,
}

fn physical(logical: u32, scale: f64) -> u32 {
    (f64::from(logical) * scale).round().max(1.0) as u32
}

fn validate(width: u32, height: u32, scale: f64) -> Result<(u32, u32), UiError> {
    let (pw, ph) = (physical(width, scale), physical(height, scale));
    let scale_ok = (MIN_SCALE..=MAX_SCALE).contains(&scale);
    let size_ok = width > 0 && height > 0 && pw <= MAX_EDGE && ph <= MAX_EDGE;
    if scale_ok && size_ok {
        return Ok((pw, ph));
    }
    Err(UiError::InvalidSize {
        width,
        height,
        scale,
    })
}

fn color_scheme(theme: &Theme) -> ColorScheme {
    match theme.mode {
        Mode::Dark => ColorScheme::Dark,
        Mode::Light => ColorScheme::Light,
    }
}

impl UiSurface {
    /// Builds a surface from `html` (a full document or a body fragment) and author `css` sheets.
    ///
    /// Theme tokens are applied to the root element; `blitz_fixups_css()` is
    /// appended after `css`.
    pub fn new(html: &str, css: &[&str], options: SurfaceOptions) -> Result<Self, UiError> {
        let (pw, ph) = validate(options.width, options.height, options.scale)?;
        let shared = Arc::new(SharedState::default());
        let config = document::config(&options, (pw, ph), &shared)?;
        let source = document::assemble(html, css);
        let mut doc = HtmlDocument::from_html(&source, config);
        doc.handle_messages();
        let mut surface = Self {
            doc,
            renderer: VelloCpuImageRenderer::new(pw, ph),
            shared,
            theme: options.theme.clone(),
            width: options.width,
            height: options.height,
            scale: options.scale,
            epoch: Instant::now(),
            dirty: true,
            frame: render::FrameBuffers::default(),
            pointer: input::PointerState::default(),
            events: VecDeque::new(),
            focus_before_leave: None,
        };
        surface.apply_theme_to_root();
        Ok(surface)
    }

    /// Logical size in CSS pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Device scale factor.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Size of the pixel buffer [`render`](Self::render) fills, in physical pixels.
    pub fn buffer_size(&self) -> (u32, u32) {
        (
            physical(self.width, self.scale),
            physical(self.height, self.scale),
        )
    }

    /// Smallest valid stride in bytes for [`render`](Self::render).
    pub fn min_stride(&self) -> usize {
        self.buffer_size().0 as usize * BYTES_PER_PIXEL
    }

    /// Changes the logical size; the next render fills a buffer of [`buffer_size`](Self::buffer_size).
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), UiError> {
        self.reconfigure(width, height, self.scale)
    }

    /// Changes the device scale factor (fractional values allowed).
    pub fn set_scale(&mut self, scale: f64) -> Result<(), UiError> {
        self.reconfigure(self.width, self.height, scale)
    }

    fn reconfigure(&mut self, width: u32, height: u32, scale: f64) -> Result<(), UiError> {
        let (pw, ph) = validate(width, height, scale)?;
        let unchanged = (width, height) == (self.width, self.height) && scale == self.scale;
        if unchanged {
            return Ok(());
        }
        (self.width, self.height, self.scale) = (width, height, scale);
        self.doc.set_viewport(Viewport::new(
            pw,
            ph,
            scale as f32,
            color_scheme(&self.theme),
        ));
        self.renderer.resize(pw, ph);
        self.frame.invalidate();
        self.mark_dirty();
        Ok(())
    }

    /// Replaces the theme tokens of the live document and marks it dirty.
    pub fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.apply_theme_to_root();
        let (pw, ph) = self.buffer_size();
        self.doc.set_viewport(Viewport::new(
            pw,
            ph,
            self.scale as f32,
            color_scheme(theme),
        ));
        self.mark_dirty();
    }

    /// The theme currently applied.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Forces the next render to draw and report the whole buffer as damaged.
    ///
    /// Use after the compositor discarded the surface contents (re-map, new configure).
    pub fn force_full_redraw(&mut self) {
        self.frame.invalidate();
        self.dirty = true;
    }

    /// Forces the next [`render`](Self::render) to draw.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// True when a call to [`render`](Self::render) would produce a new frame.
    ///
    /// Covers explicit changes, hover/focus restyles, caret blink requests and
    /// running CSS transitions. A surface that returns false costs nothing.
    pub fn needs_render(&self) -> bool {
        self.dirty || self.shared.redraw_requested() || self.doc.is_animating()
    }

    /// True while CSS transitions or animations run; the host should keep its frame loop alive.
    pub fn is_animating(&self) -> bool {
        self.doc.is_animating()
    }

    /// True when a text input has focus (the host may want to enable IME or an on-screen keyboard).
    pub fn wants_text_input(&self) -> bool {
        self.shared.ime_enabled()
    }

    /// Text most recently copied inside the document (the host owns the real clipboard).
    pub fn clipboard_text(&self) -> String {
        self.shared.clipboard_text()
    }

    /// Whole-buffer rectangle, handy for the first damage of a new buffer.
    pub fn full_damage(&self) -> PixelRect {
        let (width, height) = self.buffer_size();
        PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    pub(crate) fn animation_seconds(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Re-resolves styles and layout so geometry queries are current.
    pub(crate) fn ensure_layout(&mut self) {
        let now = self.animation_seconds();
        self.doc.resolve(now);
    }
}

#[cfg(test)]
mod tests;
