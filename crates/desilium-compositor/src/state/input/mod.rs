mod grab;
mod shortcuts;

pub use grab::{BTN_LEFT, WindowGrab};
pub use shortcuts::ShortcutTable;

use crate::launch;
use crate::state::DesiliumState;
use crate::state::layers::{ABOVE_WINDOWS, BELOW_WINDOWS};
use desilium_config_types::Action;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point as LogicalPoint, SERIAL_COUNTER};
use tracing::warn;

/// Scroll wheel notches are reported in 1/120 steps; 15 logical pixels per notch is the common default.
const PIXELS_PER_NOTCH: f64 = 15.0;
const V120_STEP: f64 = 120.0;

impl DesiliumState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        self.note_activity();
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard::<I>(event),
            InputEvent::PointerMotionAbsolute { event } => self.on_pointer_motion::<I>(event),
            InputEvent::PointerButton { event } => self.on_pointer_button::<I>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<I>(event),
            _ => {}
        }
    }

    fn on_keyboard<I: InputBackend>(&mut self, event: I::KeyboardKeyEvent) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let pressed = event.state() == KeyState::Pressed;
        let action = keyboard.input(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            Event::time_msec(&event),
            |state, modifiers, handle| {
                if !pressed {
                    return FilterResult::Forward;
                }
                match state.shortcuts.lookup(modifiers, &handle.raw_syms()) {
                    Some(action) => FilterResult::Intercept(action),
                    None => FilterResult::Forward,
                }
            },
        );
        if let Some(action) = action {
            self.run_action(action);
        }
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::CloseFocusedWindow => self.close_focused_window(),
            Action::LaunchTerminal => self.launch_terminal(),
            Action::Quit => self.loop_signal.stop(),
            #[allow(unreachable_patterns)] // more actions arrive with the window-action work
            other => self.run_window_action(other),
        }
    }

    fn launch_terminal(&self) {
        match launch::find_terminal(self.settings.terminal.as_deref()) {
            Some(program) => launch::spawn(&program, &self.socket_name),
            None => warn!("no terminal emulator found; set one in the settings"),
        }
    }

    fn on_pointer_motion<I: InputBackend>(&mut self, event: I::PointerMotionAbsoluteEvent) {
        let Some(output_geometry) = self
            .space
            .outputs()
            .next()
            .and_then(|output| self.space.output_geometry(output))
        else {
            return;
        };
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location =
            event.position_transformed(output_geometry.size) + output_geometry.loc.to_f64();
        self.decoration_pointer_moved(location);
        let under = if self.decoration_covers(location) && !self.layer_covers(location) {
            None
        } else {
            self.surface_under(location)
        };
        self.request_redraw();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_button<I: InputBackend>(&mut self, event: I::PointerButtonEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location = pointer.current_location();
        if !self.layer_covers(location)
            && self.decoration_pointer_button(location, event.button_code(), event.state())
        {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
            self.focus_window_under(pointer.current_location());
        }
        pointer.button(
            self,
            &ButtonEvent {
                button: event.button_code(),
                state: event.state(),
                serial,
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
    }

    /// Click-to-focus: raises the window under the pointer, or clears focus on empty desktop.
    fn focus_window_under(&mut self, location: LogicalPoint<f64, Logical>) {
        if self.focus_layer_at(location) {
            return;
        }
        let under = self
            .space
            .element_under(location)
            .and_then(|(window, _)| self.window_id_of(window));
        match under {
            Some(id) => {
                self.wm.focus(id);
                self.sync_space();
            }
            None => self.wm.clear_focus(),
        }
        self.apply_focus();
    }

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let discrete = event.amount_v120(axis);
            let amount = event
                .amount(axis)
                .unwrap_or_else(|| discrete.unwrap_or(0.0) * PIXELS_PER_NOTCH / V120_STEP);
            if amount != 0.0 {
                frame = frame.value(axis, amount);
            }
            if let (true, Some(discrete)) = (amount != 0.0, discrete) {
                frame = frame.v120(axis, discrete as i32);
            }
            if event.source() == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                frame = frame.stop(axis);
            }
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    /// Pointer target: overlay and top layers, then windows, then bottom and background layers.
    pub fn surface_under(
        &self,
        location: LogicalPoint<f64, Logical>,
    ) -> Option<(WlSurface, LogicalPoint<f64, Logical>)> {
        self.layer_surface_under(&ABOVE_WINDOWS, location)
            .map(|hit| (hit.surface, hit.location))
            .or_else(|| self.window_surface_under(location))
            .or_else(|| {
                self.layer_surface_under(&BELOW_WINDOWS, location)
                    .map(|hit| (hit.surface, hit.location))
            })
    }

    /// Whether a layer surface above the windows sits under the pointer.
    fn layer_covers(&self, location: LogicalPoint<f64, Logical>) -> bool {
        self.layer_surface_under(&ABOVE_WINDOWS, location).is_some()
    }

    fn window_surface_under(
        &self,
        location: LogicalPoint<f64, Logical>,
    ) -> Option<(WlSurface, LogicalPoint<f64, Logical>)> {
        let (window, origin) = self.space.element_under(location)?;
        let (surface, offset) =
            window.surface_under(location - origin.to_f64(), WindowSurfaceType::ALL)?;
        Some((surface, (offset + origin).to_f64()))
    }
}
