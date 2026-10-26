use crate::state::input::WindowGrab;
use crate::state::{DesiliumState, WindowRequest};
use desilium_wm::{Edges, Size, WindowId};
use smithay::delegate_xdg_shell;
use smithay::desktop::{PopupKind, Window, find_popup_root_surface, get_popup_toplevel_coords};
use smithay::input::Seat;
use smithay::input::pointer::{Focus, GrabStartData};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::{ResizeEdge, State};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use tracing::{info, warn};

/// Cascade size hint until the client commits its real buffer.
const DEFAULT_WINDOW_SIZE: Size = Size { w: 800, h: 600 };

impl XdgShellHandler for DesiliumState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let id = self.wm.open_window(DEFAULT_WINDOW_SIZE);
        self.initial_windows.insert(id);
        self.windows.insert(id, Window::new_wayland_window(surface));
        info!(window = id.get(), "window opened");
        self.sync_space();
        self.apply_focus();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        self.initial_windows.remove(&id);
        self.wm.close_window(id);
        self.forget_decorated(id);
        self.begin_close_motion(id);
        info!(window = id.get(), "window closed");
        if let Some(window) = self.windows.remove(&id) {
            self.space.unmap_elem(&window);
        }
        self.apply_focus();
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!(%error, "cannot track popup");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: WlSeat, serial: Serial) {
        let Some((start_data, id)) = self.window_grab_context(&seat, surface.wl_surface(), serial)
        else {
            return;
        };
        let Some(rect) = self.wm.rect(id) else {
            return;
        };
        self.start_grab(&seat, serial, WindowGrab::moving(start_data, id, rect.loc));
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: WlSeat,
        serial: Serial,
        edges: ResizeEdge,
    ) {
        let Some((start_data, id)) = self.window_grab_context(&seat, surface.wl_surface(), serial)
        else {
            return;
        };
        let edges = edges_from(edges);
        let Some(initial) = self.wm.begin_resize(id, edges) else {
            return;
        };
        surface.with_pending_state(|state| state.states.set(State::Resizing));
        surface.send_pending_configure();
        self.start_grab(
            &seat,
            serial,
            WindowGrab::resizing(start_data, id, edges, initial),
        );
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.handle_window_request(&surface, WindowRequest::Maximize);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        self.handle_window_request(&surface, WindowRequest::Unmaximize);
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        self.handle_window_request(&surface, WindowRequest::Fullscreen);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.handle_window_request(&surface, WindowRequest::Unfullscreen);
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        self.handle_window_request(&surface, WindowRequest::Minimize);
    }

    fn title_changed(&mut self, _surface: ToplevelSurface) {
        self.request_redraw();
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // Popup grabs (click-outside dismissal) arrive with the input-focus work.
    }
}

delegate_xdg_shell!(DesiliumState);

fn edges_from(edge: ResizeEdge) -> Edges {
    let (left, right, top, bottom) = match edge {
        ResizeEdge::Top => (false, false, true, false),
        ResizeEdge::Bottom => (false, false, false, true),
        ResizeEdge::Left => (true, false, false, false),
        ResizeEdge::Right => (false, true, false, false),
        ResizeEdge::TopLeft => (true, false, true, false),
        ResizeEdge::TopRight => (false, true, true, false),
        ResizeEdge::BottomLeft => (true, false, false, true),
        ResizeEdge::BottomRight => (false, true, false, true),
        _ => (false, false, false, false),
    };
    Edges {
        left,
        right,
        top,
        bottom,
    }
}

impl DesiliumState {
    /// A move/resize request is only honoured while the requesting client holds the pointer button.
    fn window_grab_context(
        &self,
        seat: &WlSeat,
        surface: &WlSurface,
        serial: Serial,
    ) -> Option<(GrabStartData<Self>, WindowId)> {
        let seat = Seat::from_resource(seat)?;
        let pointer = seat.get_pointer()?;
        if !pointer.has_grab(serial) {
            return None;
        }
        let start_data = pointer.grab_start_data()?;
        let focus: &WlSurface = &start_data.focus.as_ref()?.0;
        if !focus.id().same_client_as(&surface.id()) {
            return None;
        }
        Some((start_data, self.window_id_for_surface(surface)?))
    }

    fn start_grab(&mut self, seat: &WlSeat, serial: Serial, grab: WindowGrab) {
        let Some(pointer) = Seat::<Self>::from_resource(seat).and_then(|seat| seat.get_pointer())
        else {
            return;
        };
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    pub(super) fn handle_popup_commit(&mut self, surface: &WlSurface) {
        self.popups.commit(surface);
        let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) else {
            return;
        };
        if popup.is_initial_configure_sent() {
            return;
        }
        if let Err(error) = popup.send_configure() {
            warn!(%error, "initial popup configure failed");
        }
    }

    /// Keeps popups inside the output by letting the positioner flip/slide them.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Some(window) = self
            .window_id_for_surface(&root)
            .and_then(|id| self.windows.get(&id))
        else {
            return;
        };
        let Some(output_geometry) = self
            .space
            .outputs()
            .next()
            .and_then(|output| self.space.output_geometry(output))
        else {
            return;
        };
        let Some(window_geometry) = self.space.element_geometry(window) else {
            return;
        };
        let mut target = output_geometry;
        target.loc -= get_popup_toplevel_coords(&kind);
        target.loc -= window_geometry.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target)
        });
    }
}
