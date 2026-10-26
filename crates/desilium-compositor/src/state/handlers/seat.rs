use crate::state::DesiliumState;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
    set_data_device_focus,
};
use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::wayland::tablet_manager::TabletSeatHandler;
use smithay::{delegate_cursor_shape, delegate_data_device, delegate_output, delegate_seat};

impl SeatHandler for DesiliumState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.set_client_cursor(image);
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display_handle.get_client(surface.id()).ok());
        set_primary_focus(&self.display_handle, seat, client.clone());
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

impl SelectionHandler for DesiliumState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for DesiliumState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for DesiliumState {}
impl ServerDndGrabHandler for DesiliumState {}
impl OutputHandler for DesiliumState {}
// Required by cursor-shape-v1; tablet tools are not supported yet.
impl TabletSeatHandler for DesiliumState {}

delegate_seat!(DesiliumState);
delegate_cursor_shape!(DesiliumState);
delegate_data_device!(DesiliumState);
delegate_output!(DesiliumState);
