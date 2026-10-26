use crate::error::CompositorError;
use crate::state::DesiliumState;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::wayland::compositor::CompositorClientState;
use smithay::wayland::socket::ListeningSocketSource;
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

/// Per-client data every connection is created with.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// Set once when the connection is accepted; decides who sees the shell protocol.
    shell_trusted: AtomicBool,
}

impl ClientState {
    /// Whether the client may bind the private shell protocol.
    pub fn shell_trusted(&self) -> bool {
        self.shell_trusted.load(Ordering::Relaxed)
    }

    /// Records the outcome of the trust check.
    pub fn set_shell_trusted(&self, trusted: bool) {
        self.shell_trusted.store(trusted, Ordering::Relaxed);
    }
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        info!(?client_id, "client connected");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        info!(?client_id, ?reason, "client disconnected");
    }
}

/// Opens the Wayland socket and feeds client requests into the event loop.
pub fn init(
    handle: &LoopHandle<'static, DesiliumState>,
    display: Display<DesiliumState>,
) -> Result<OsString, CompositorError> {
    let socket = ListeningSocketSource::new_auto()?;
    let socket_name = socket.socket_name().to_os_string();

    handle
        .insert_source(socket, |stream, _, state| {
            match state
                .display_handle
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                Ok(client) => state.classify_client(&client),
                Err(error) => warn!(%error, "rejected client connection"),
            }
        })
        .map_err(|error| error.error)?;

    handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                // SAFETY: the display is owned by this event source and is never dropped while it is registered.
                let result = unsafe { display.get_mut().dispatch_clients(state) };
                if let Err(error) = result {
                    warn!(%error, "dispatching client requests failed");
                }
                state.shell_sync();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| error.error)?;

    info!(socket = ?socket_name, "listening for Wayland clients");
    Ok(socket_name)
}
