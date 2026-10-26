//! wlr-layer-shell requests: panels, docks, launchers and lock screens.

use crate::state::DesiliumState;
use smithay::delegate_layer_shell;
use smithay::desktop::{LayerSurface, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::{
    Layer, LayerSurface as ShellLayerSurface, LayerSurfaceData, WlrLayerShellHandler,
    WlrLayerShellState,
};
use tracing::warn;

impl WlrLayerShellHandler for DesiliumState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: ShellLayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let Some(output) = self.requested_output(output.as_ref()) else {
            warn!(%namespace, "no output for a layer surface; closing it");
            surface.send_close();
            return;
        };
        let layer = LayerSurface::new(surface, namespace);
        if let Err(error) = layer_map_for_output(&output).map_layer(&layer) {
            warn!(%error, "mapping a layer surface failed");
            return;
        }
        self.arrange_layers();
    }

    fn layer_destroyed(&mut self, surface: ShellLayerSurface) {
        if self.unmap_layer_surface(&surface) {
            self.arrange_layers();
        }
        // Focus may have sat on the destroyed surface; hand it back to the window layer.
        self.apply_focus();
    }
}

impl DesiliumState {
    /// The output a client asked for, falling back to the compositor's output.
    fn requested_output(&self, requested: Option<&WlOutput>) -> Option<Output> {
        requested
            .and_then(Output::from_resource)
            .or_else(|| self.layer_output())
    }

    /// Runs on every commit of a layer surface: arranges it, sends the first configure, updates focus.
    pub(crate) fn handle_layer_commit(&mut self, root: &WlSurface) {
        let Some(layer) = self.layer_for_root(root) else {
            return;
        };
        self.arrange_layers();
        if !initial_configure_sent(root) {
            layer.layer_surface().send_configure();
        }
        self.sync_keyboard_focus();
    }
}

/// The initial configure must follow the first commit; this tells whether it went out yet.
fn initial_configure_sent(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states
            .data_map
            .get::<LayerSurfaceData>()
            .and_then(|data| data.lock().ok())
            .is_some_and(|data| data.initial_configure_sent)
    })
}

delegate_layer_shell!(DesiliumState);
