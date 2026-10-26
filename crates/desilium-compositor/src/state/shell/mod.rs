//! Server side of the private `desilium-shell-v1` protocol: window and workspace state for the
//! desktop shell, plus the requests it may make. See `docs/shell-protocol.md`.
//!
//! State flows one way: [`DesiliumState::shell_sync`] builds a [`ShellSnapshot`] and sends each
//! bound client the difference to the snapshot it saw last. It is the only place that decides
//! what to send, so no feature has to know about the shell.

mod collect;
mod control;
mod diff;
mod handlers;
mod minimize;
mod send;
mod snapshot;
#[cfg(test)]
mod tests;
mod trust;

use crate::state::DesiliumState;
use crate::state::layers::LayerEffects;
use desilium_protocols::server::desilium_shell_manager_v1::DesiliumShellManagerV1;
use desilium_protocols::server::desilium_toplevel_handle_v1::DesiliumToplevelHandleV1;
use desilium_wm::WindowId;
use minimize::MinimizeTarget;
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::{Client, DisplayHandle, Resource};
use snapshot::ShellSnapshot;
use std::collections::HashMap;
use tracing::{info, warn};

/// Version of the shell protocol this compositor implements.
const PROTOCOL_VERSION: u32 = 1;

/// One bound manager and what its client has already been told.
struct ShellClient {
    manager: DesiliumShellManagerV1,
    /// `None` until the first update, which then carries the full state.
    last: Option<ShellSnapshot>,
    handles: HashMap<WindowId, DesiliumToplevelHandleV1>,
}

/// Everything the shell protocol keeps in the compositor state.
pub struct ShellState {
    // Held to keep the global registered.
    _global: GlobalId,
    clients: Vec<ShellClient>,
    effects: LayerEffects,
    minimize_targets: HashMap<WindowId, MinimizeTarget>,
}

impl ShellState {
    /// Registers the manager global; only trusted clients can see it (see [`trust`]).
    pub fn new(dh: &DisplayHandle) -> Self {
        if trust::development_mode() {
            warn!(
                "{} is set: every client may bind the shell protocol",
                trust::ALLOW_ANY_VAR
            );
        }
        Self {
            _global: dh
                .create_global::<DesiliumState, DesiliumShellManagerV1, ()>(PROTOCOL_VERSION, ()),
            clients: Vec::new(),
            effects: LayerEffects::default(),
            minimize_targets: HashMap::new(),
        }
    }
}

impl DesiliumState {
    /// Sends every bound shell client what changed since its last update.
    ///
    /// Cheap when no shell is bound. Called after client requests are dispatched and after each
    /// presented frame, which together cover client, input and animation driven changes.
    pub(crate) fn shell_sync(&mut self) {
        if !self.shell.effects.is_empty() {
            self.shell.effects.prune();
        }
        if self.shell.clients.is_empty() {
            return;
        }
        let snapshot = self.shell_snapshot();
        self.shell
            .minimize_targets
            .retain(|id, _| snapshot.windows.iter().any(|window| window.id == *id));
        let dh = self.display_handle.clone();
        for client in &mut self.shell.clients {
            send::publish(&dh, client, &snapshot);
        }
    }

    /// Tells every bound shell that a compositor key binding it owns was pressed (for example
    /// `desilium_protocols::shortcut::TOGGLE_LAUNCHER`). Returns whether any shell received it,
    /// so callers can fall back to starting one.
    pub fn shell_shortcut(&mut self, name: &str) -> bool {
        let mut delivered = false;
        for client in self.shell.clients.iter().filter(|c| c.manager.is_alive()) {
            client.manager.shortcut(name.to_owned());
            delivered = true;
        }
        delivered
    }

    /// Decides whether `client` may see the shell global and records it on the client.
    pub(crate) fn classify_client(&self, client: &Client) {
        let trusted = trust::client_is_trusted(client, &self.display_handle);
        if let Some(data) = client.get_data::<crate::state::ClientState>() {
            data.set_shell_trusted(trusted);
        }
        if trusted {
            info!("a client may use the shell protocol");
        }
    }

    pub(crate) fn shell_effects(&self) -> &LayerEffects {
        &self.shell.effects
    }
}
