//! xdg-decoration: every toplevel gets server-side decorations.

use crate::state::DesiliumState;
use smithay::delegate_xdg_decoration;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

impl DesiliumState {
    /// Announces server-side mode and marks the window as decorated.
    ///
    /// Architecture decision: the compositor always draws the chrome, whatever the client prefers,
    /// so every window looks the same.
    fn force_server_side(&mut self, toplevel: &ToplevelSurface) {
        toplevel.with_pending_state(|state| state.decoration_mode = Some(Mode::ServerSide));
        self.set_server_decorated(toplevel);
        self.sync_space();
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl XdgDecorationHandler for DesiliumState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.force_server_side(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        self.force_server_side(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.force_server_side(&toplevel);
    }
}

delegate_xdg_decoration!(DesiliumState);
