//! Protocol state and per-window bookkeeping of the xdg window host.

use std::{num::NonZeroU32, time::Instant};

use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    reexports::{
        client::{
            QueueHandle,
            protocol::{
                wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_shm, wl_surface::WlSurface,
            },
        },
        protocols::wp::{
            cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
            fractional_scale::v1::client::{
                wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                wp_fractional_scale_v1::WpFractionalScaleV1,
            },
            viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
        },
    },
    registry::RegistryState,
    seat::{SeatState, pointer::cursor_shape::CursorShapeManager},
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{DecorationMode, Window},
        },
    },
    shm::{Shm, slot::SlotPool},
};

use super::{WindowConfig, WindowEvent};
use crate::{
    CursorHint, HoverTarget, Modifiers, UiSurface,
    wayland::{config::SurfaceId, keys::KeyRepeat, state::FRACTIONAL_DENOMINATOR},
};

/// Initial shm pool size; the pool grows on demand.
const INITIAL_POOL_BYTES: usize = 64 * 1024;

/// Who draws the title bar, as negotiated with the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decorations {
    /// The compositor draws the title bar and borders (the desired outcome).
    Server,
    /// The client is expected to draw its own; this host draws none.
    Client,
}

/// Size the document should take for a configure: the compositor's choice, else the current size.
///
/// Never smaller than `min`, so a layout designed for the minimum is not squeezed below it.
pub(super) fn resolve_size(
    suggested: (Option<NonZeroU32>, Option<NonZeroU32>),
    current: (u32, u32),
    min: Option<(u32, u32)>,
) -> (u32, u32) {
    let width = suggested.0.map_or(current.0, NonZeroU32::get);
    let height = suggested.1.map_or(current.1, NonZeroU32::get);
    let (min_w, min_h) = min.unwrap_or((1, 1));
    (width.max(min_w).max(1), height.max(min_h).max(1))
}

impl From<DecorationMode> for Decorations {
    fn from(mode: DecorationMode) -> Self {
        match mode {
            DecorationMode::Server => Decorations::Server,
            DecorationMode::Client => Decorations::Client,
        }
    }
}

/// One managed toplevel window.
pub(super) struct Entry {
    pub id: SurfaceId,
    pub window: Window,
    pub ui: UiSurface,
    pub min_size: Option<(u32, u32)>,
    pool: SlotPool,
    pub viewport: Option<WpViewport>,
    pub fractional: Option<WpFractionalScaleV1>,
    /// True once a `wp_fractional_scale` event arrived (buffer scale stays 1 then).
    pub fractional_active: bool,
    pub configured: bool,
    pub frame_pending: bool,
    pub hover: Option<HoverTarget>,
    pub decorations: Decorations,
}

impl Entry {
    pub(super) fn new(
        id: SurfaceId,
        window: Window,
        ui: UiSurface,
        config: &WindowConfig,
        shm: &Shm,
    ) -> Result<Self, crate::UiError> {
        let pool = SlotPool::new(INITIAL_POOL_BYTES, shm)
            .map_err(|error| crate::UiError::Wayland(error.to_string()))?;
        Ok(Self {
            id,
            window,
            ui,
            min_size: config.min_size,
            pool,
            viewport: None,
            fractional: None,
            fractional_active: false,
            configured: false,
            frame_pending: false,
            hover: None,
            decorations: Decorations::Client,
        })
    }

    pub(super) fn wants_draw(&self) -> bool {
        self.configured && !self.frame_pending && self.ui.needs_render()
    }

    /// Tells the compositor how buffer pixels map to logical pixels.
    fn apply_scale(&self) {
        let surface = self.window.wl_surface();
        let (width, height) = self.ui.size();
        match (&self.viewport, self.fractional_active) {
            (Some(viewport), true) => viewport.set_destination(width as i32, height as i32),
            _ => surface.set_buffer_scale(self.ui.scale().round().max(1.0) as i32),
        }
    }

    /// Renders into a fresh shm buffer and commits it. Identical frames are not committed.
    pub(super) fn draw(&mut self, qh: &QueueHandle<State>) -> bool {
        let (width, height) = self.ui.buffer_size();
        let stride = self.ui.min_stride();
        let created = self.pool.create_buffer(
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        );
        let Ok((buffer, canvas)) = created else {
            tracing::error!("shm buffer allocation failed");
            return false;
        };
        let stats = match self.ui.render(canvas, stride) {
            Ok(stats) => stats,
            Err(error) => {
                tracing::error!(%error, "render failed");
                return false;
            }
        };
        let Some(damage) = stats.damage else {
            return false;
        };
        let surface = self.window.wl_surface();
        self.apply_scale();
        surface.damage_buffer(
            damage.x as i32,
            damage.y as i32,
            damage.width as i32,
            damage.height as i32,
        );
        surface.frame(qh, surface.clone());
        if let Err(error) = buffer.attach_to(surface) {
            tracing::error!(%error, "attaching buffer failed");
            return false;
        }
        self.window.commit();
        self.frame_pending = true;
        true
    }
}

/// Pointer/keyboard bookkeeping shared by all windows.
#[derive(Default)]
pub(super) struct InputState {
    pub pointer: Option<WlPointer>,
    pub keyboard: Option<WlKeyboard>,
    pub cursor_device: Option<WpCursorShapeDeviceV1>,
    pub pointer_focus: Option<SurfaceId>,
    pub enter_serial: u32,
    pub keyboard_focus: Option<SurfaceId>,
    pub modifiers: Modifiers,
    pub repeat: KeyRepeat,
    /// Cursor shape last sent for the focused window.
    pub sent_cursor: Option<CursorHint>,
}

/// Everything the sctk handler traits operate on.
pub(super) struct State {
    pub registry: RegistryState,
    pub outputs: OutputState,
    pub seats: SeatState,
    pub compositor: CompositorState,
    pub xdg_shell: XdgShell,
    pub shm: Shm,
    pub viewporter: Option<WpViewporter>,
    pub fractional_manager: Option<WpFractionalScaleManagerV1>,
    pub cursor_shapes: Option<CursorShapeManager>,
    pub entries: Vec<Entry>,
    pub next_id: u32,
    pub events: Vec<WindowEvent>,
    pub input: InputState,
    pub now: Instant,
    /// While true, key presses are only reported as [`WindowEvent::Key`], never typed into the document.
    pub capture_keys: bool,
    /// Buffers committed so far (diagnostics, idle-cost measurements).
    pub frames: u64,
}

impl State {
    pub(super) fn entry_mut(&mut self, id: SurfaceId) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    pub(super) fn entry_for_surface(&mut self, surface: &WlSurface) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.window.wl_surface() == surface)
    }

    /// Draws every window that is dirty and not waiting for a frame callback.
    pub(super) fn draw_ready(&mut self, qh: &QueueHandle<State>) {
        for entry in &mut self.entries {
            if entry.wants_draw() && entry.draw(qh) {
                self.frames += 1;
            }
        }
    }

    /// Moves document events and hover changes of all windows into the host queue.
    pub(super) fn collect_ui_events(&mut self) {
        for entry in &mut self.entries {
            let window = entry.id;
            for event in entry.ui.take_events() {
                self.events.push(WindowEvent::Ui { window, event });
            }
            let hover = entry.ui.hover_target();
            if hover != entry.hover {
                entry.hover = hover.clone();
                self.events.push(WindowEvent::Hover {
                    window,
                    target: hover,
                });
            }
        }
    }

    /// Applies a fractional scale (in 120ths) to a window's document.
    pub(super) fn apply_fractional(&mut self, surface: &WlSurface, scale_120ths: u32) {
        let Some(entry) = self.entry_for_surface(surface) else {
            return;
        };
        entry.fractional_active = entry.viewport.is_some();
        let factor = f64::from(scale_120ths) / FRACTIONAL_DENOMINATOR;
        if let Err(error) = entry.ui.set_scale(factor) {
            tracing::warn!(%error, "ignoring fractional scale");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(value: u32) -> Option<NonZeroU32> {
        NonZeroU32::new(value)
    }

    #[test]
    fn compositor_size_wins_and_missing_axes_keep_the_current_size() {
        assert_eq!(resolve_size((nz(800), nz(600)), (1, 1), None), (800, 600));
        assert_eq!(resolve_size((None, nz(600)), (400, 300), None), (400, 600));
        assert_eq!(resolve_size((None, None), (400, 300), None), (400, 300));
    }

    #[test]
    fn minimum_size_is_enforced() {
        let min = Some((640, 480));
        assert_eq!(resolve_size((nz(100), nz(900)), (1, 1), min), (640, 900));
    }

    #[test]
    fn size_is_never_zero() {
        assert_eq!(resolve_size((None, None), (0, 0), None), (1, 1));
    }
}
