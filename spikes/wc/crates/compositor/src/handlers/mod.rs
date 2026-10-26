mod compositor;
mod xdg_shell;

use crate::Smallvil;

//
// Wl Seat
//

use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};

impl SeatHandler for Smallvil {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Smallvil> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_status = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

impl PointerConstraintsHandler for Smallvil {}

//
// Wl Data Device
//

impl SelectionHandler for Smallvil {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Smallvil {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for Smallvil {}
impl WaylandDndGrabHandler for Smallvil {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let ptr = seat.get_pointer().unwrap();
                let start_data = ptr.grab_start_data().unwrap();

                // create a dnd grab to start the operation
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                ptr.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => {
                // smallvil lacks touch handling
                source.cancel();
            }
        }
    }
}

//
// Wl Output & Xdg Output
//

impl OutputHandler for Smallvil {}

smithay::delegate_dispatch2!(Smallvil);

// We explicitly negotiate client-side title bars until server decorations exist.
impl smithay::wayland::shell::xdg::decoration::XdgDecorationHandler for Smallvil {
    fn new_decoration(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        self.request_mode(surface, smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ClientSide);
    }
    fn request_mode(
        &mut self,
        surface: smithay::wayland::shell::xdg::ToplevelSurface,
        _mode: smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    ) {
        surface.with_pending_state(|s| s.decoration_mode = Some(smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode::ClientSide));
        if surface.is_initial_configure_sent() {
            surface.send_pending_configure();
        }
    }
    fn unset_mode(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        self.new_decoration(surface);
    }
}

impl smithay::wayland::dmabuf::DmabufHandler for Smallvil {
    fn dmabuf_state(&mut self) -> &mut smithay::wayland::dmabuf::DmabufState {
        &mut self.dmabuf_state
    }
    fn dmabuf_imported(
        &mut self,
        _global: &smithay::wayland::dmabuf::DmabufGlobal,
        dmabuf: smithay::backend::allocator::dmabuf::Dmabuf,
        notifier: smithay::wayland::dmabuf::ImportNotifier,
    ) {
        use smithay::backend::renderer::ImportDma;
        if self
            .backend
            .as_mut()
            .is_some_and(|backend| backend.renderer().import_dmabuf(&dmabuf, None).is_ok())
        {
            let _ = notifier.successful::<Self>();
        } else {
            notifier.failed();
        }
    }
}

// Required by cursor-shape's tablet branch; no tablet devices are advertised.
impl smithay::input::tablet::TabletSeatHandler for Smallvil {
    type ToolFocus = WlSurface;
}
