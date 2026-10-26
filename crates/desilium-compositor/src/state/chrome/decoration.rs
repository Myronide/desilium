//! Server-side decoration state and pointer handling.

use super::snap::SnapState;
use crate::decoration::{Button, Chrome, Frame, Hit, Interaction, Metrics, hit_test};
use crate::state::DesiliumState;
use crate::state::input::{BTN_LEFT, WindowGrab};
use desilium_cursor::CursorKind;
use desilium_wm::{Edges, Point, Rect, Size, SizeLimits, WindowId, WindowState};
use smithay::backend::input::ButtonState;
use smithay::input::pointer::{Focus, GrabStartData};
use smithay::utils::{Logical, Point as LogicalPoint, SERIAL_COUNTER};
use smithay::wayland::shell::xdg::ToplevelSurface;
use std::collections::HashSet;

/// Pointer travel before a title bar press turns into a move.
pub const DRAG_THRESHOLD: f64 = 4.0;

/// Runtime state of the window chrome (hover, pressed button, which windows are decorated).
#[derive(Default)]
pub struct DecorationState {
    interaction: Interaction,
    /// Windows whose client asked for (and got) server-side decorations.
    decorated: HashSet<WindowId>,
    /// Resize cursor currently forced by hovering a border.
    cursor: Option<CursorKind>,
    /// Drag-to-snap preview shown while a title bar drag hovers a snap zone.
    pub(super) snap: SnapState,
}

impl DecorationState {
    /// Button hover/press bookkeeping, read by the scene.
    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }
}

fn whole(location: LogicalPoint<f64, Logical>) -> Point {
    Point {
        x: location.x.floor() as i32,
        y: location.y.floor() as i32,
    }
}

/// Cursor shape for dragging `edges`; `None` when no edge is set.
pub fn cursor_for_edges(edges: Edges) -> Option<CursorKind> {
    let kind = match (edges.left, edges.right, edges.top, edges.bottom) {
        (true, _, true, _) => CursorKind::ResizeNw,
        (_, true, true, _) => CursorKind::ResizeNe,
        (true, _, _, true) => CursorKind::ResizeSw,
        (_, true, _, true) => CursorKind::ResizeSe,
        (true, _, _, _) => CursorKind::ResizeW,
        (_, true, _, _) => CursorKind::ResizeE,
        (_, _, true, _) => CursorKind::ResizeN,
        (_, _, _, true) => CursorKind::ResizeS,
        _ => return None,
    };
    Some(kind)
}

impl DesiliumState {
    /// Chrome sizes derived from the current theme.
    pub fn chrome_metrics(&self) -> Metrics {
        Metrics::from_theme(&self.theme)
    }

    /// Marks a toplevel as server-side decorated (called by the xdg-decoration handler).
    pub(crate) fn set_server_decorated(&mut self, surface: &ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        self.decoration.decorated.insert(id);
    }

    /// Forgets all chrome state of a closed window.
    pub(crate) fn forget_decorated(&mut self, id: WindowId) {
        self.decoration.decorated.remove(&id);
        if self.decoration.interaction.forget(id) {
            self.request_redraw();
        }
    }

    /// How much chrome `id` wears right now.
    pub fn chrome_of(&self, id: WindowId) -> Chrome {
        if !self.decoration.decorated.contains(&id) {
            return Chrome::None;
        }
        match self.wm.window_state(id) {
            Some(WindowState::Normal | WindowState::Minimized) => Chrome::Windowed,
            Some(WindowState::Maximized) => Chrome::Maximized,
            Some(WindowState::Fullscreen) | None => Chrome::None,
        }
    }

    /// Frame of `id` for the rectangle the window manager gave it.
    pub fn frame_of(&self, id: WindowId, outer: Rect) -> Frame {
        Frame::new(&self.chrome_metrics(), self.chrome_of(id), outer)
    }

    /// Size the client surface gets for a window frame of `frame`.
    pub fn client_size_for_frame(&self, id: WindowId, frame: Size) -> Size {
        let insets = self.chrome_metrics().insets(self.chrome_of(id));
        insets.client_size(frame)
    }

    /// Frame size that wraps a client surface of `client`.
    pub fn frame_size_for_client(&self, id: WindowId, client: Size) -> Size {
        let insets = self.chrome_metrics().insets(self.chrome_of(id));
        insets.frame_size(client)
    }

    /// Client size limits translated to frame limits (chrome added, floor for the buttons).
    pub(crate) fn frame_limits(&self, id: WindowId, client: SizeLimits) -> SizeLimits {
        let metrics = self.chrome_metrics();
        let chrome = self.chrome_of(id);
        let insets = metrics.insets(chrome);
        let floor = metrics.min_frame_size(chrome);
        let min = insets.frame_size(client.min);
        let grow = |limit: i32, extra: i32| {
            if limit > 0 {
                limit.saturating_add(extra)
            } else {
                0
            }
        };
        SizeLimits {
            min: Size {
                w: min.w.max(floor.w),
                h: min.h.max(floor.h),
            },
            max: Size {
                w: grow(client.max.w, insets.horizontal()),
                h: grow(client.max.h, insets.vertical()),
            },
        }
    }

    /// Topmost window frame under `location` and what it hits there.
    fn chrome_hit_at(&self, location: LogicalPoint<f64, Logical>) -> Option<(WindowId, Hit)> {
        let point = whole(location);
        self.wm
            .placements()
            .into_iter()
            .rev()
            .find_map(|placement| {
                let frame = self.frame_of(placement.id, placement.rect);
                let hit = hit_test(&frame, point)?;
                Some((placement.id, hit))
            })
    }

    /// Called on every pointer motion before client focus is resolved.
    pub fn decoration_pointer_moved(&mut self, location: LogicalPoint<f64, Logical>) {
        if self
            .seat
            .get_pointer()
            .is_some_and(|pointer| pointer.is_grabbed())
        {
            return;
        }
        let hit = self.chrome_hit_at(location);
        let hover = match hit {
            Some((id, Hit::Button(button))) => Some((id, button)),
            _ => None,
        };
        if self.decoration.interaction.set_hover(hover) {
            self.request_redraw();
        }
        let cursor = match hit {
            Some((_, Hit::ResizeBorder(edges))) => cursor_for_edges(edges),
            _ => None,
        };
        if cursor != self.decoration.cursor {
            self.decoration.cursor = cursor;
            self.set_cursor_override(cursor);
        }
    }

    /// Returns true when the press/release was consumed by window chrome.
    pub fn decoration_pointer_button(
        &mut self,
        location: LogicalPoint<f64, Logical>,
        button: u32,
        state: ButtonState,
    ) -> bool {
        let hit = self.chrome_hit_at(location);
        if button != BTN_LEFT {
            return self.consume_other_button(hit, state);
        }
        match state {
            ButtonState::Pressed => self.chrome_press(location, hit),
            ButtonState::Released => self.chrome_release(hit),
        }
    }

    /// True when `location` lies on chrome, so no client surface may receive pointer focus there.
    pub fn decoration_covers(&self, location: LogicalPoint<f64, Logical>) -> bool {
        self.chrome_hit_at(location)
            .is_some_and(|(_, hit)| hit.is_chrome())
    }

    fn consume_other_button(&mut self, hit: Option<(WindowId, Hit)>, state: ButtonState) -> bool {
        let Some((id, _)) = hit.filter(|(_, hit)| hit.is_chrome()) else {
            return false;
        };
        if state == ButtonState::Pressed {
            self.focus_and_raise(id);
        }
        true
    }

    fn chrome_press(
        &mut self,
        location: LogicalPoint<f64, Logical>,
        hit: Option<(WindowId, Hit)>,
    ) -> bool {
        let Some((id, hit)) = hit else {
            return false;
        };
        match hit {
            Hit::Client => false,
            Hit::Button(button) => {
                self.focus_and_raise(id);
                self.decoration.interaction.press((id, button));
                self.request_redraw();
                true
            }
            Hit::TitleBar => self.title_bar_press(id, location),
            Hit::ResizeBorder(edges) => {
                self.focus_and_raise(id);
                self.start_chrome_resize(id, edges, location);
                false
            }
        }
    }

    fn title_bar_press(&mut self, id: WindowId, location: LogicalPoint<f64, Logical>) -> bool {
        self.focus_and_raise(id);
        let now = self.start_time.elapsed();
        if self
            .decoration
            .interaction
            .register_click(id, now, whole(location))
        {
            self.toggle_maximize_window(id);
            return true;
        }
        let start_data = grab_start(location);
        self.set_window_grab(WindowGrab::move_when_dragged(start_data, id));
        // The grab sees the press itself, so the release can end it.
        false
    }

    fn chrome_release(&mut self, hit: Option<(WindowId, Hit)>) -> bool {
        let over = match hit {
            Some((id, Hit::Button(button))) => Some((id, button)),
            _ => None,
        };
        let (activate, changed) = self.decoration.interaction.release(over);
        if changed {
            self.request_redraw();
        }
        if let Some((id, button)) = activate {
            self.activate_button(id, button);
        }
        changed
    }

    fn activate_button(&mut self, id: WindowId, button: Button) {
        match button {
            Button::Close => self.close_window(id),
            Button::Maximize => self.toggle_maximize_window(id),
            Button::Minimize => self.minimize_window(id),
        }
    }

    fn focus_and_raise(&mut self, id: WindowId) {
        if self.wm.focus(id) {
            self.sync_space();
        }
        self.apply_focus();
    }

    fn start_chrome_resize(
        &mut self,
        id: WindowId,
        edges: Edges,
        location: LogicalPoint<f64, Logical>,
    ) {
        let Some(initial) = self.wm.begin_resize(id, edges) else {
            return;
        };
        let grab = WindowGrab::resizing(grab_start(location), id, edges, initial);
        self.set_window_grab(grab);
    }

    fn set_window_grab(&mut self, grab: WindowGrab) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        pointer.set_grab(self, grab, SERIAL_COUNTER.next_serial(), Focus::Clear);
    }

    /// Detaches `id` from a maximized/snapped layout at the start of a title bar drag and tells
    /// its client. Returns the new frame position.
    pub(crate) fn begin_window_move(&mut self, id: WindowId, pointer: Point) -> Option<Point> {
        let before = self.wm.window_state(id);
        let loc = self.wm.begin_move(id, pointer)?;
        self.decoration_configure_client(id);
        if before != self.wm.window_state(id) {
            self.apply_focus();
        }
        self.sync_space();
        Some(loc)
    }

    /// Sends the size and maximized/fullscreen flags the window manager now wants.
    fn decoration_configure_client(&self, id: WindowId) {
        let (Some(rect), Some(state), Some(window)) = (
            self.wm.rect(id),
            self.wm.window_state(id),
            self.windows.get(&id),
        ) else {
            return;
        };
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let client = self.client_size_for_frame(id, rect.size);
        toplevel.with_pending_state(|pending| {
            use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
            pending.size = Some((client.w, client.h).into());
            pending.states.unset(State::Maximized);
            pending.states.unset(State::Fullscreen);
            if state == WindowState::Maximized {
                pending.states.set(State::Maximized);
            }
            if state == WindowState::Fullscreen {
                pending.states.set(State::Fullscreen);
            }
        });
        toplevel.send_pending_configure();
    }
}

fn grab_start(location: LogicalPoint<f64, Logical>) -> GrabStartData<DesiliumState> {
    GrabStartData {
        focus: None,
        button: BTN_LEFT,
        location,
    }
}
