use crate::state::DesiliumState;
use crate::state::chrome::DRAG_THRESHOLD;
use desilium_wm::{Edges, Point, Rect, Size, SnapZone, WindowId, resized_size};
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
    fn finish(&self, data: &mut DesiliumState) {
        match &self.mode {
            Mode::Move {
                snap: Some(zone), ..
            } => data.snap_window_to(self.id, *zone),
            Mode::Move { snap: None, .. } => {}
            Mode::Resize { last_size, .. } => {
                data.configure_resize(self.id, *last_size, false);
                data.wm.end_resize(self.id);
            }
        }
    }
}

impl DesiliumState {
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
