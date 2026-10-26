use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::state::Smallvil;

impl Smallvil {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time(&event);

                let keycode = event.key_code();
                let pressed = event.state() == KeyState::Pressed;
                let swallowed_release = !pressed && self.swallowed_keys.remove(&keycode);
                let action = self.seat.get_keyboard().unwrap().input::<String, _>(
                    self,
                    keycode,
                    event.state(),
                    serial,
                    time,
                    |state, mods, keys| {
                        if swallowed_release {
                            return FilterResult::Intercept(String::new());
                        }
                        if pressed
                            && let Some(action) = keys
                                .raw_syms()
                                .first()
                                .and_then(|key| state.shortcut(mods, *key))
                        {
                            state.swallowed_keys.insert(keycode);
                            return FilterResult::Intercept(action);
                        }
                        FilterResult::Forward
                    },
                );
                if let Some(action) = action
                    && !action.is_empty()
                {
                    self.action(&action);
                }
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                if self.policy.settings.focus == wc_core::FocusPolicy::Sloppy
                    && !pointer.is_grabbed()
                {
                    let target = self.space.element_under(pos).map(|(w, _)| w.clone());
                    if let Some(w) = target
                        && self.focused().as_ref() != Some(&w)
                    {
                        self.focus_window(&w, self.policy.settings.raise_on_focus);
                    }
                }
                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.focus_window(&window, true);
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().unwrap().send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time(),
                    },
                );
                if button_state == ButtonState::Pressed && keyboard.modifier_state().logo {
                    let target = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, _)| w.clone());
                    if let (Some(window), Some(start_data)) = (target, pointer.grab_start_data()) {
                        use smithay::input::pointer::Focus;
                        if button == 0x110 {
                            self.prepare_interactive_move(&window);
                            let initial_window_location =
                                self.space.element_location(&window).unwrap();
                            pointer.set_grab(
                                self,
                                crate::grabs::MoveSurfaceGrab {
                                    start_data,
                                    window,
                                    initial_window_location,
                                },
                                serial,
                                Focus::Clear,
                            );
                        } else if button == 0x111 {
                            use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
                            self.policy.placements.remove(&window);
                            self.policy.restore.remove(&window);
                            let top = window.toplevel().unwrap();
                            top.with_pending_state(|s| {
                                s.states.unset(xdg_toplevel::State::Maximized);
                                s.states.set(xdg_toplevel::State::Resizing);
                            });
                            top.send_pending_configure();
                            let rect = smithay::utils::Rectangle::new(
                                self.space.element_location(&window).unwrap(),
                                window.geometry().size,
                            );
                            let grab = crate::grabs::ResizeSurfaceGrab::start(
                                start_data,
                                window,
                                xdg_toplevel::ResizeEdge::BottomRight.into(),
                                rect,
                            );
                            pointer.set_grab(self, grab, serial, Focus::Clear);
                        }
                    }
                }
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.
                });
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}
