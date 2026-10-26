use crate::state::{ClientState, DesiliumState};
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{delegate_compositor, delegate_shm};

impl CompositorHandler for DesiliumState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // pureline:allow(PL-RS-004) every client is inserted with ClientState in state/listener.rs
        let data = client.get_data::<ClientState>().expect("ClientState");
        &data.compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            self.handle_window_commit(&root_surface(surface));
        }
        self.handle_popup_commit(surface);
        self.handle_layer_commit(&root_surface(surface));
        self.request_redraw();
    }
}

fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

impl BufferHandler for DesiliumState {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for DesiliumState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(DesiliumState);
delegate_shm!(DesiliumState);
