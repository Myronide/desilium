use crate::state::DesiliumState;
use crate::state::chrome::DRAG_THRESHOLD;
use desilium_wm::{Edges, Point, Rect, Size, SnapZone, WindowId, WindowState, resized_size};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
    GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
    GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
    RelativeMotionEvent,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point as LogicalPoint};

/// Linux input-event-codes.h BTN_LEFT.
pub const BTN_LEFT: u32 = 0x110;

/// Whether a grab may keep acting on its window. The window must still exist, sit on the desktop
/// in view and not be minimized; maximized and fullscreen windows stay live because a title bar
/// drag restores them.
pub fn grab_target_is_live(
    desktop: Option<usize>,
    active: usize,
    state: Option<WindowState>,
) -> bool {
    desktop == Some(active) && state.is_some_and(|state| state != WindowState::Minimized)
}

enum Mode {
    Move {
        initial: Point,
        /// False for a title bar press that has not travelled far enough to count as a drag.
        armed: bool,
        /// Snap zone under the pointer; released into when the button goes up.
        snap: Option<SnapZone>,
    },
    Resize {
        edges: Edges,
        initial: Rect,
        last_size: Size,
    },
}

/// Interactive move or resize that follows the pointer until the left button is released.
pub struct WindowGrab {
    start_data: GrabStartData<DesiliumState>,
    id: WindowId,
    mode: Mode,
}

impl WindowGrab {
    pub fn moving(start_data: GrabStartData<DesiliumState>, id: WindowId, initial: Point) -> Self {
        Self {
            start_data,
            id,
            mode: Mode::Move {
                initial,
                armed: true,
                snap: None,
            },
        }
    }

    /// Title bar press: the window only leaves its layout once the pointer really drags.
    pub fn move_when_dragged(start_data: GrabStartData<DesiliumState>, id: WindowId) -> Self {
        Self {
            start_data,
            id,
            mode: Mode::Move {
                initial: Point::default(),
                armed: false,
                snap: None,
            },
        }
    }

    pub fn resizing(
        start_data: GrabStartData<DesiliumState>,
        id: WindowId,
        edges: Edges,
        initial: Rect,
    ) -> Self {
        Self {
            start_data,
            id,
            mode: Mode::Resize {
                edges,
                initial,
                last_size: initial.size,
            },
        }
    }

    fn drag(
        &mut self,
        data: &mut DesiliumState,
        delta: LogicalPoint<f64, Logical>,
        pointer: LogicalPoint<f64, Logical>,
    ) {
        match &mut self.mode {
            Mode::Move {
                initial,
                armed,
                snap,
            } => {
                if !*armed {
                    let travelled = delta.x.hypot(delta.y);
                    if travelled < DRAG_THRESHOLD {
                        return;
                    }
                    let pointer = Point {
                        x: self.start_data.location.x.floor() as i32,
                        y: self.start_data.location.y.floor() as i32,
                    };
                    let Some(loc) = data.begin_window_move(self.id, pointer) else {
                        return;
                    };
                    *initial = loc;
                    *armed = true;
                }
                let target = Point {
                    x: (initial.x as f64 + delta.x).round() as i32,
                    y: (initial.y as f64 + delta.y).round() as i32,
                };
                if data.wm.move_to(self.id, target) {
                    data.sync_space();
                }
                *snap = data.update_snap_preview(self.id, whole_point(pointer));
            }
            Mode::Resize {
                edges,
                initial,
                last_size,
            } => {
                let limits = data.frame_limits(self.id, data.size_limits(self.id));
                *last_size = resized_size(initial.size, *edges, delta.x, delta.y, limits);
                data.configure_resize(self.id, *last_size, true);
            }
        }
    }

    /// Commits the gesture on release: a resize is final, a move snaps if it hovered a zone.
    /// A grab whose window is no longer in view ends without snapping, but a resize still
    /// leaves its window's resize phase so the window is not stuck mid-resize on its desktop.
    fn finish(&self, data: &mut DesiliumState) {
        let live = data.grab_target_live(self.id);
        match &self.mode {
            Mode::Move {
                snap: Some(zone), ..
            } if live && data.settings.snap_enabled => data.snap_window_to(self.id, *zone),
            Mode::Move { .. } => {}
            Mode::Resize { last_size, .. } => {
                data.configure_resize(self.id, *last_size, false);
                data.end_window_resize(self.id);
            }
        }
    }
}

impl DesiliumState {
    /// Whether an interactive grab on `id` may keep acting (see [`grab_target_is_live`]).
    fn grab_target_live(&self, id: WindowId) -> bool {
        grab_target_is_live(
            self.wm.desktop_of(id),
            self.wm.active(),
            self.wm.window_state(id),
        )
    }

    /// Ends the resize bookkeeping of `id` on whichever desktop holds it.
    fn end_window_resize(&mut self, id: WindowId) {
        self.wm.with_window_manager(id, |wm| wm.end_resize(id));
    }

    /// `frame` is the whole window frame; the client gets the part inside the chrome.
    fn configure_resize(&self, id: desilium_wm::WindowId, frame: Size, resizing: bool) {
        let size = self.client_size_for_frame(id, frame);
        let Some(toplevel) = self.windows.get(&id).and_then(|window| window.toplevel()) else {
            return;
        };
        toplevel.with_pending_state(|state| {
            if resizing {
                state.states.set(State::Resizing);
            } else {
                state.states.unset(State::Resizing);
            }
            state.size = Some((size.w, size.h).into());
        });
        toplevel.send_pending_configure();
    }
}

impl PointerGrab<DesiliumState> for WindowGrab {
    fn motion(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        _focus: Option<(WlSurface, LogicalPoint<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // Desktop switches, closes and minimizes reach the grab here; it ends instead of acting.
        if !data.grab_target_live(self.id) {
            handle.unset_grab(self, data, event.serial, event.time, true);
            self.finish(data);
            return;
        }
        // While the grab is active no client has pointer focus.
        handle.motion(data, None, event);
        self.drag(
            data,
            event.location - self.start_data.location,
            event.location,
        );
    }

    fn relative_motion(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        focus: Option<(WlSurface, LogicalPoint<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().contains(&BTN_LEFT) {
            return;
        }
        handle.unset_grab(self, data, event.serial, event.time, true);
        self.finish(data);
    }

    fn axis(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
    ) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut DesiliumState,
        handle: &mut PointerInnerHandle<'_, DesiliumState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<DesiliumState> {
        &self.start_data
    }

    /// Runs when the grab ends for any reason; the preview must never outlive the grab.
    fn unset(&mut self, data: &mut DesiliumState) {
        data.clear_snap_preview();
    }
}

/// The pointer as whole logical pixels, the unit the window manager works in.
fn whole_point(location: LogicalPoint<f64, Logical>) -> Point {
    Point {
        x: location.x.floor() as i32,
        y: location.y.floor() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grab_on_the_visible_desktop_stays_live_in_every_visible_state() {
        for state in [
            WindowState::Normal,
            WindowState::Maximized,
            WindowState::Fullscreen,
        ] {
            assert!(grab_target_is_live(Some(1), 1, Some(state)), "{state:?}");
        }
    }

    #[test]
    fn grab_ends_when_the_window_is_on_another_desktop() {
        assert!(!grab_target_is_live(Some(0), 1, Some(WindowState::Normal)));
    }

    #[test]
    fn grab_ends_when_the_window_was_destroyed() {
        assert!(!grab_target_is_live(None, 0, None));
        assert!(!grab_target_is_live(Some(0), 0, None));
    }

    #[test]
    fn grab_ends_when_the_window_is_minimized() {
        assert!(!grab_target_is_live(
            Some(0),
            0,
            Some(WindowState::Minimized)
        ));
    }
}
