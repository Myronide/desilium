//! Theme loading and the per-frame bitmap cache for themed cursors.

use super::clock::AnimationClock;
use super::layout::{image_scale, snapped_origin};
use crate::render::scene::SceneContext;
use desilium_cursor::{CursorImage, CursorKind, CursorTheme};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Transform};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::warn;

/// Upper bound of cached bitmaps; animated themes need a few dozen at most.
const MAX_CACHED_FRAMES: usize = 256;

type Element = MemoryRenderBufferRenderElement<GlesRenderer>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FrameKey {
    kind: CursorKind,
    scale: u32,
    /// Address of the frame inside the theme; frames live as long as the theme does.
    frame: usize,
}

struct LoadedTheme {
    name: Option<String>,
    size: u32,
    theme: CursorTheme,
}

/// Loaded theme, uploaded bitmaps and the animation clock.
pub struct ThemedCursors {
    theme: Option<LoadedTheme>,
    buffers: HashMap<FrameKey, MemoryRenderBuffer>,
    clock: AnimationClock,
    epoch: Instant,
}

impl ThemedCursors {
    pub fn new() -> Self {
        Self {
            theme: None,
            buffers: HashMap::new(),
            clock: AnimationClock::default(),
            epoch: Instant::now(),
        }
    }

    /// Called when no themed cursor is visible, so an animation restarts on the next show.
    pub fn hide(&mut self) {
        self.clock.stop();
    }

    /// The element for `kind` with the pointer at `location`, or `None` if the upload failed.
    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        ctx: &SceneContext<'_>,
        kind: CursorKind,
        location: Point<f64, Logical>,
    ) -> Option<Element> {
        self.sync_theme(ctx);
        let theme = &self.theme.as_ref()?.theme;
        let now_ms = u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        let scale = image_scale(ctx.scale);
        let image = theme.image(kind, scale, self.clock.elapsed(kind, now_ms));
        let key = FrameKey {
            kind,
            scale,
            frame: std::ptr::from_ref(image) as usize,
        };
        if theme.is_animated(kind) {
            self.clock.observe_frame(key.frame, now_ms);
            self.schedule_wake(ctx, image.delay_ms, now_ms);
        }
        if self.buffers.len() >= MAX_CACHED_FRAMES {
            self.buffers.clear();
        }
        let buffer = self
            .buffers
            .entry(key)
            .or_insert_with(|| upload(image, scale));
        let origin = snapped_origin(
            (location.x, location.y),
            ctx.scale,
            (image.hotspot_x, image.hotspot_y),
            scale,
        );
        let created =
            Element::from_buffer(renderer, origin, buffer, None, None, None, Kind::Cursor);
        created
            .inspect_err(|error| warn!(%error, "cursor upload failed"))
            .ok()
    }

    fn schedule_wake(&self, ctx: &SceneContext<'_>, delay_ms: u32, now_ms: u64) {
        if let Some(wake_ms) = self.clock.wake_after_ms(delay_ms, now_ms) {
            ctx.state
                .schedule_cursor_redraw(Duration::from_millis(wake_ms));
        }
    }

    /// Reloads the theme when the appearance settings changed since the last frame.
    fn sync_theme(&mut self, ctx: &SceneContext<'_>) {
        let appearance = ctx.state.appearance();
        let current = self.theme.as_ref().is_some_and(|loaded| {
            loaded.name == appearance.cursor_theme && loaded.size == appearance.cursor_size
        });
        if current {
            return;
        }
        self.buffers.clear();
        self.clock.stop();
        self.theme = Some(LoadedTheme {
            name: appearance.cursor_theme.clone(),
            size: appearance.cursor_size,
            theme: CursorTheme::load(appearance.cursor_theme.as_deref(), appearance.cursor_size),
        });
    }
}

fn upload(image: &CursorImage, scale: u32) -> MemoryRenderBuffer {
    let size = (image.width as i32, image.height as i32);
    MemoryRenderBuffer::from_slice(
        &image.rgba_premultiplied,
        Fourcc::Abgr8888,
        size,
        scale as i32,
        Transform::Normal,
        None,
    )
}
