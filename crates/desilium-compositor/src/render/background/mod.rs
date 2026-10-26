//! Desktop background: the theme gradient, with a wallpaper image crossfaded over it.
//!
//! The wallpaper is decoded and fitted to the output on a worker thread ([`worker`]); the render
//! path only polls for the result and wraps it in a texture element. Until the image is ready the
//! theme gradient is shown, so a slow or broken file never blocks or blanks the desktop.

mod fade;
mod layers;
mod request;
mod worker;

use super::effects::{GradientBackground, GradientStop, MAX_GRADIENT_STOPS, Rgba};
use super::elements::DesiliumElement;
use super::scene::{SceneContext, rgba};
use fade::Fade;
use layers::{Layer, Transition};
use request::{CacheKey, WallpaperSource, checked_size, expand_home, parse_mode, parse_override};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Rectangle};
use std::cell::RefCell;
use std::time::Duration;
use tracing::{debug, warn};
use worker::{Job, Poll};

/// DEVELOPMENT OVERRIDE: `DESILIUM_WALLPAPER=path[:mode]` is used only while the appearance
/// setting names no wallpaper. It exists so the renderer can be tested before settings are wired.
const DEV_OVERRIDE_ENV: &str = "DESILIUM_WALLPAPER";

/// GL-bound background resources: the wallpaper cache and its worker.
pub struct Resources {
    inner: RefCell<State>,
    dev_override: Option<WallpaperSource>,
}

#[derive(Default)]
struct State {
    shown: Option<Layer>,
    transition: Option<Transition>,
    pending: Option<Job>,
    /// Last key that failed to load; not retried (and not logged again) until the key changes.
    failed: Option<CacheKey>,
    /// Output size last reported as above the cap, so the warning appears once.
    capped: Option<(i32, i32)>,
}

impl Resources {
    pub fn new(_renderer: &mut GlesRenderer) -> Self {
        let dev_override = std::env::var(DEV_OVERRIDE_ENV)
            .ok()
            .and_then(|value| parse_override(&value));
        Self {
            inner: RefCell::default(),
            dev_override,
        }
    }
}

/// Pushes the background element(s); called last, so it is drawn below everything.
pub fn push(out: &mut Vec<DesiliumElement>, renderer: &mut GlesRenderer, ctx: &SceneContext<'_>) {
    let now = ctx.state.start_time.elapsed();
    let mut state = ctx.resources.background.inner.borrow_mut();
    let target = desired_key(ctx, &mut state);
    let animate = ctx.state.appearance().animations;
    state.update(target, now, animate);
    let covered = state.push_layers(out, renderer, ctx, now);
    if state.is_busy() {
        // No wake-up handle reaches the worker, so poll through the redraw loop while it runs.
        ctx.state.request_redraw();
    }
    drop(state);
    if !covered {
        push_gradient(out, ctx);
    }
}

fn desired_key(ctx: &SceneContext<'_>, state: &mut State) -> Option<(CacheKey, WallpaperSource)> {
    let source = configured_source(ctx)?;
    let physical = ctx
        .area
        .size
        .to_f64()
        .to_physical(ctx.scale)
        .to_i32_round::<i32>();
    let Some(size) = checked_size(physical.w, physical.h) else {
        let dims = (physical.w, physical.h);
        if state.capped != Some(dims) {
            warn!(
                width = dims.0,
                height = dims.1,
                "output too large for a wallpaper texture; using the gradient"
            );
            state.capped = Some(dims);
        }
        return None;
    };
    state.capped = None;
    let key = CacheKey::new(&source, size, ctx.scale);
    Some((key, source))
}

fn configured_source(ctx: &SceneContext<'_>) -> Option<WallpaperSource> {
    let appearance = ctx.state.appearance();
    let configured = appearance
        .wallpaper
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty());
    match configured {
        Some(path) => Some(WallpaperSource {
            path: expand_home(path),
            mode: parse_mode(&appearance.wallpaper_mode),
        }),
        None => ctx.resources.background.dev_override.clone(),
    }
}

impl State {
    fn target_key(&self) -> Option<&CacheKey> {
        match &self.transition {
            Some(transition) => transition.to.as_ref().map(|layer| &layer.key),
            None => self.shown.as_ref().map(|layer| &layer.key),
        }
    }

    fn is_busy(&self) -> bool {
        self.pending.is_some() || self.transition.is_some()
    }

    fn update(
        &mut self,
        target: Option<(CacheKey, WallpaperSource)>,
        now: Duration,
        animate: bool,
    ) {
        self.settle(now);
        self.collect_result(now, animate);
        match target {
            None => self.show_gradient(now, animate),
            Some((key, source)) => self.request(key, source),
        }
    }

    fn settle(&mut self, now: Duration) {
        let finished = self
            .transition
            .as_ref()
            .is_some_and(|t| t.fade.is_finished(now));
        if !finished {
            return;
        }
        self.shown = self.transition.take().and_then(|transition| transition.to);
    }

    fn collect_result(&mut self, now: Duration, animate: bool) {
        let Some(job) = self.pending.as_ref() else {
            return;
        };
        let Poll::Done(outcome) = job.poll() else {
            return;
        };
        let Some(job) = self.pending.take() else {
            return;
        };
        match outcome {
            Ok(rendered) => match Layer::new(job.key.clone(), rendered) {
                Some(layer) => self.begin(Some(layer), now, animate),
                None => self.failed = Some(job.key.clone()),
            },
            Err(error) => {
                warn!(path = %job.key.path.display(), %error, "wallpaper unavailable; keeping the current background");
                self.failed = Some(job.key.clone());
            }
        }
    }

    fn request(&mut self, key: CacheKey, source: WallpaperSource) {
        let known = self.target_key() == Some(&key)
            || self.pending.as_ref().is_some_and(|job| job.key == key)
            || self.failed.as_ref() == Some(&key);
        if known {
            return;
        }
        match Job::spawn(key.clone(), source) {
            Ok(job) => self.pending = Some(job),
            Err(error) => {
                warn!(%error, "could not start the wallpaper worker");
                self.failed = Some(key);
            }
        }
    }

    fn show_gradient(&mut self, now: Duration, animate: bool) {
        self.pending = None;
        self.failed = None;
        if self.target_key().is_some() {
            self.begin(None, now, animate);
        }
    }

    /// Starts a fade to `to`; keeps only the outgoing and the incoming layer alive.
    fn begin(&mut self, to: Option<Layer>, now: Duration, animate: bool) {
        let from = match self.transition.take() {
            Some(transition) => transition.to,
            None => self.shown.take(),
        };
        self.transition = Some(Transition {
            from,
            to,
            fade: Fade::start(now, animate),
        });
        self.settle(now);
    }

    /// Pushes wallpaper elements; returns true when they fully cover the gradient.
    fn push_layers(
        &self,
        out: &mut Vec<DesiliumElement>,
        renderer: &mut GlesRenderer,
        ctx: &SceneContext<'_>,
        now: Duration,
    ) -> bool {
        let Some(transition) = &self.transition else {
            let Some(layer) = &self.shown else {
                return false;
            };
            return push_layer(out, renderer, ctx, layer, 1.0);
        };
        let alphas = transition.alphas(now);
        let mut covered = false;
        if let (Some(layer), Some(alpha)) = (&transition.to, alphas.to) {
            push_layer(out, renderer, ctx, layer, alpha);
        }
        if let (Some(layer), Some(alpha)) = (&transition.from, alphas.from) {
            covered = push_layer(out, renderer, ctx, layer, alpha);
        }
        covered
    }
}

/// Returns true when the layer was pushed fully opaque.
fn push_layer(
    out: &mut Vec<DesiliumElement>,
    renderer: &mut GlesRenderer,
    ctx: &SceneContext<'_>,
    layer: &Layer,
    alpha: f32,
) -> bool {
    let size = ctx.area.size;
    let element = MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        (0.0, 0.0),
        &layer.buffer,
        Some(alpha),
        None,
        Some(size),
        Kind::Unspecified,
    );
    match element {
        Ok(element) => {
            out.push(DesiliumElement::from(element));
            alpha >= 1.0
        }
        Err(error) => {
            debug!(%error, "wallpaper texture upload failed");
            false
        }
    }
}

fn push_gradient(out: &mut Vec<DesiliumElement>, ctx: &SceneContext<'_>) {
    let Some(effects) = ctx.resources.effects.as_ref() else {
        return;
    };
    let gradient = &ctx.state.theme().background.gradient;
    let stops: Vec<GradientStop> = gradient
        .stops
        .iter()
        .take(MAX_GRADIENT_STOPS)
        .map(|stop| GradientStop {
            position: stop.position,
            color: rgba(stop.color),
        })
        .collect();
    let area: Rectangle<i32, Logical> = ctx.area;
    let background =
        GradientBackground::two_stop(area, Rgba::BLACK, Rgba::BLACK, gradient.angle_deg)
            .scale(ctx.scale)
            .stops(stops);
    out.extend(
        effects
            .gradient_background(&background)
            .map(DesiliumElement::from),
    );
}
