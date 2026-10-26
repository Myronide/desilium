//! smithay-client-toolkit handler implementations and the small protocols it does not wrap.

use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::{
        client::{
            Connection, Dispatch, QueueHandle,
            protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface::WlSurface},
        },
        protocols::wp::{
            fractional_scale::v1::client::{
                wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                wp_fractional_scale_v1::{self, WpFractionalScaleV1},
            },
            viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{
            KeyEvent as SctkKey, KeyboardHandler, Keymap, Modifiers as SctkModifiers, RepeatInfo,
        },
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    },
    shm::{Shm, ShmHandler},
};

use super::{
    config::HostEvent,
    input_map::{cursor_shape, to_button, to_modifiers, wheel_delta},
    keys,
    state::{Entry, FRACTIONAL_DENOMINATOR, InputState, State},
};

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &WlSurface,
        factor: i32,
    ) {
        let Some(entry) = self.entry_for_surface(surface) else {
            return;
        };
        if entry.fractional_active {
            return;
        }
        if let Err(error) = entry.ui.set_scale(f64::from(factor)) {
            tracing::warn!(%error, "ignoring scale change");
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        if let Some(entry) = self.entry_for_surface(surface) {
            entry.frame_pending = false;
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        let Some(position) = self.entries.iter().position(|entry| &entry.layer == layer) else {
            return;
        };
        let entry = self.entries.remove(position);
        self.events.push(HostEvent::Closed(entry.id));
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(entry) = self.entries.iter_mut().find(|entry| &entry.layer == layer) else {
            return;
        };
        let (current_w, current_h) = entry.ui.size();
        let (width, height) = configure.new_size;
        let size = (
            if width == 0 { current_w } else { width },
            if height == 0 { current_h } else { height },
        );
        if let Err(error) = entry.ui.resize(size.0, size.1) {
            tracing::warn!(%error, "ignoring configure size");
        }
        entry.configured = true;
        entry.frame_pending = false;
        entry.ui.force_full_redraw();
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer if self.input.pointer.is_none() => self.bind_pointer(qh, &seat),
            Capability::Keyboard if self.input.keyboard.is_none() => self.bind_keyboard(qh, &seat),
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer => {
                self.input.pointer = None;
                self.input.cursor_device = None;
                self.input.pointer_focus = None;
            }
            Capability::Keyboard => {
                if let Some(keyboard) = self.input.keyboard.take() {
                    keyboard.release();
                }
                self.input.keyboard_focus = None;
                self.input.repeat.release(None);
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

fn apply_pointer_event(entry: &mut Entry, input: &mut InputState, event: &PointerEvent) {
    let (x, y) = event.position;
    match &event.kind {
        PointerEventKind::Enter { serial } => {
            input.pointer_focus = Some(entry.id);
            input.enter_serial = *serial;
            input.sent_cursor = None;
            entry.ui.pointer_move(x, y);
        }
        PointerEventKind::Leave { .. } => {
            if input.pointer_focus == Some(entry.id) {
                input.pointer_focus = None;
            }
            entry.ui.pointer_leave();
        }
        PointerEventKind::Motion { .. } => entry.ui.pointer_move(x, y),
        PointerEventKind::Press { button, .. } => {
            if let Some(button) = to_button(*button) {
                entry.ui.pointer_down(x, y, button);
            }
        }
        PointerEventKind::Release { button, .. } => {
            if let Some(button) = to_button(*button) {
                entry.ui.pointer_up(x, y, button);
            }
        }
        PointerEventKind::Axis {
            horizontal,
            vertical,
            ..
        } => {
            entry.ui.wheel(x, y, wheel_delta(horizontal, vertical));
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| e.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            apply_pointer_event(entry, &mut self.input, event);
        }
        self.sync_cursor();
    }
}

impl State {
    fn bind_pointer(&mut self, qh: &QueueHandle<Self>, seat: &wl_seat::WlSeat) {
        let Ok(pointer) = self.seats.get_pointer(qh, seat) else {
            tracing::warn!("seat refused a pointer");
            return;
        };
        self.input.cursor_device = self
            .cursor_shapes
            .as_ref()
            .map(|manager| manager.get_shape_device(&pointer, qh));
        self.input.pointer = Some(pointer);
    }

    fn bind_keyboard(&mut self, qh: &QueueHandle<Self>, seat: &wl_seat::WlSeat) {
        match self.seats.get_keyboard(qh, seat, None) {
            Ok(keyboard) => self.input.keyboard = Some(keyboard),
            Err(error) => tracing::warn!(%error, "seat refused a keyboard"),
        }
    }

    /// Sends the cursor shape of the hovered element to the compositor when it changed.
    pub(super) fn sync_cursor(&mut self) {
        let Some(device) = self.input.cursor_device.as_ref() else {
            return;
        };
        let Some(id) = self.input.pointer_focus else {
            return;
        };
        let Some(entry) = self.entries.iter().find(|entry| entry.id == id) else {
            return;
        };
        let hint = entry.ui.cursor_hint();
        if self.input.sent_cursor == Some(hint) {
            return;
        }
        device.set_shape(self.input.enter_serial, cursor_shape(hint));
        self.input.sent_cursor = Some(hint);
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.layer.wl_surface() == surface)
        else {
            return;
        };
        let id = entry.id;
        self.input.keyboard_focus = Some(id);
        entry.ui.focus_changed(true);
        self.events.push(HostEvent::KeyboardFocus {
            surface: id,
            focused: true,
        });
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &WlSurface,
        _: u32,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.layer.wl_surface() == surface)
        {
            entry.ui.focus_changed(false);
            let surface = entry.id;
            self.events.push(HostEvent::KeyboardFocus {
                surface,
                focused: false,
            });
        }
        self.input.keyboard_focus = None;
        self.input.repeat.release(None);
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: SctkKey,
    ) {
        let Some(id) = self.input.keyboard_focus else {
            return;
        };
        let key = keys::translate(&event, self.input.modifiers, false);
        self.route_key(id, &key);
        let now = self.now;
        self.input.repeat.press(id, event.raw_code, key, now);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: SctkKey,
    ) {
        self.input.repeat.release(Some(event.raw_code));
        let Some(id) = self.input.keyboard_focus else {
            return;
        };
        let key = keys::translate(&event, self.input.modifiers, false);
        if let Some(entry) = self.entry_mut(id) {
            entry.ui.key_up(&key);
        }
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: SctkModifiers,
        _: u32,
    ) {
        self.input.modifiers = to_modifiers(&modifiers);
    }

    fn update_repeat_info(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        self.input.repeat.set_info(info);
    }

    fn update_keymap(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: Keymap<'_>,
    ) {
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

// Protocols without events or wrapped by hand.
impl Dispatch<WpViewporter, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: <WpViewporter as smithay_client_toolkit::reexports::client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as smithay_client_toolkit::reexports::client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as smithay_client_toolkit::reexports::client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, WlSurface> for State {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let Some(entry) = state.entry_for_surface(surface) else {
            return;
        };
        entry.fractional_active = entry.viewport.is_some();
        let factor = f64::from(scale) / FRACTIONAL_DENOMINATOR;
        if let Err(error) = entry.ui.set_scale(factor) {
            tracing::warn!(%error, "ignoring fractional scale");
        }
    }
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_keyboard!(State);
delegate_layer!(State);
delegate_registry!(State);
