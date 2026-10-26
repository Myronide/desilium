//! Protocol state of the host and the per-surface bookkeeping.

use std::time::Instant;

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
        wlr_layer::{LayerShell, LayerSurface},
    },
    shm::{Shm, slot::SlotPool},
};

use super::{
    config::{HostEvent, LayerConfig, SurfaceId},
    keys::KeyRepeat,
};
use crate::{CursorHint, HoverTarget, Key, KeyEvent, UiSurface};

/// Initial shm pool size; the pool grows on demand.
const INITIAL_POOL_BYTES: usize = 64 * 1024;
/// Fractional scales are sent in 120ths.
pub(super) const FRACTIONAL_DENOMINATOR: f64 = 120.0;

/// One managed layer surface.
pub(super) struct Entry {
    pub id: SurfaceId,
    pub layer: LayerSurface,
    pub ui: UiSurface,
    pub config: LayerConfig,
    pool: SlotPool,
    pub viewport: Option<WpViewport>,
    pub fractional: Option<WpFractionalScaleV1>,
    /// True once a `wp_fractional_scale` event arrived (buffer scale stays 1 then).
    pub fractional_active: bool,
    pub configured: bool,
    pub visible: bool,
    pub frame_pending: bool,
    pub hover: Option<HoverTarget>,
}

impl Entry {
    pub(super) fn new(
        id: SurfaceId,
        layer: LayerSurface,
        ui: UiSurface,
        config: LayerConfig,
        shm: &Shm,
    ) -> Result<Self, crate::UiError> {
        let pool = SlotPool::new(INITIAL_POOL_BYTES, shm)
            .map_err(|error| crate::UiError::Wayland(error.to_string()))?;
        Ok(Self {
            id,
            layer,
            ui,
            config,
            pool,
            viewport: None,
            fractional: None,
            fractional_active: false,
            configured: false,
            visible: true,
            frame_pending: false,
            hover: None,
        })
    }

    pub(super) fn wants_draw(&self) -> bool {
        self.configured && self.visible && !self.frame_pending && self.ui.needs_render()
    }

    /// Tells the compositor how buffer pixels map to logical pixels.
    fn apply_scale(&self) {
        let surface = self.layer.wl_surface();
        let (width, height) = self.ui.size();
        match (&self.viewport, self.fractional_active) {
            (Some(viewport), true) => viewport.set_destination(width as i32, height as i32),
            _ => surface.set_buffer_scale(self.ui.scale().round().max(1.0) as i32),
        }
    }

    /// Renders into a fresh shm buffer and commits it. Identical frames are not committed.
    pub(super) fn draw(&mut self, qh: &QueueHandle<super::state::State>) -> bool {
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
        let surface = self.layer.wl_surface();
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
        self.layer.commit();
        self.frame_pending = true;
        true
    }
}

/// Pointer/keyboard bookkeeping shared by all surfaces.
#[derive(Default)]
pub(super) struct InputState {
    pub pointer: Option<WlPointer>,
    pub keyboard: Option<WlKeyboard>,
    pub cursor_device: Option<WpCursorShapeDeviceV1>,
    pub pointer_focus: Option<SurfaceId>,
    pub enter_serial: u32,
    pub keyboard_focus: Option<SurfaceId>,
    pub modifiers: crate::Modifiers,
    pub repeat: KeyRepeat,
    /// Cursor shape last sent for the focused surface.
    pub sent_cursor: Option<CursorHint>,
}

/// Everything the sctk handler traits operate on.
pub(super) struct State {
    pub registry: RegistryState,
    pub outputs: OutputState,
    pub seats: SeatState,
    pub compositor: CompositorState,
    pub layer_shell: LayerShell,
    pub shm: Shm,
    pub viewporter: Option<WpViewporter>,
    pub fractional_manager: Option<WpFractionalScaleManagerV1>,
    pub cursor_shapes: Option<CursorShapeManager>,
    pub entries: Vec<Entry>,
    pub next_id: u32,
    pub events: Vec<HostEvent>,
    pub input: InputState,
    pub now: Instant,
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
            .find(|entry| entry.layer.wl_surface() == surface)
    }

    /// Sends a key press to the surface: to the document, to the application (`report_keys`), or both.
    pub(super) fn route_key(&mut self, id: SurfaceId, key: &KeyEvent) {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        if !entry.config.report_keys {
            entry.ui.key_down(key);
            return;
        }
        if !is_reserved_key(key) {
            entry.ui.key_down(key);
        }
        self.events.push(HostEvent::Key {
            surface: id,
            event: key.clone(),
        });
    }

    /// Draws every surface that is dirty and not waiting for a frame callback.
    pub(super) fn draw_ready(&mut self, qh: &QueueHandle<State>) {
        for entry in &mut self.entries {
            if entry.wants_draw() && entry.draw(qh) {
                self.frames += 1;
            }
        }
    }

    /// Moves document events and hover changes of all surfaces into the host queue.
    pub(super) fn collect_ui_events(&mut self) {
        for entry in &mut self.entries {
            let surface = entry.id;
            for event in entry.ui.take_events() {
                self.events.push(HostEvent::Ui { surface, event });
            }
            let hover = entry.ui.hover_target();
            if hover != entry.hover {
                entry.hover = hover.clone();
                self.events.push(HostEvent::Hover {
                    surface,
                    target: hover,
                });
            }
        }
    }
}

/// Keys the application handles itself on `report_keys` surfaces.
fn is_reserved_key(key: &KeyEvent) -> bool {
    let navigation = matches!(
        key.key,
        Key::Escape
            | Key::Tab
            | Key::Enter
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::PageUp
            | Key::PageDown
    );
    navigation || key.modifiers.alt || key.modifiers.logo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifiers;

    #[test]
    fn navigation_and_alt_chords_are_reserved_typing_is_not() {
        assert!(is_reserved_key(&KeyEvent::named(Key::Escape)));
        assert!(is_reserved_key(&KeyEvent::named(Key::ArrowDown)));
        assert!(!is_reserved_key(&KeyEvent::named(Key::Backspace)));
        assert!(!is_reserved_key(&KeyEvent::character("a")));
        let mut alt_one = KeyEvent::character("1");
        alt_one.modifiers = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert!(is_reserved_key(&alt_one));
        let mut ctrl_a = KeyEvent::character("a");
        ctrl_a.modifiers.ctrl = true;
        assert!(!is_reserved_key(&ctrl_a));
    }
}
