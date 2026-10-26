//! Dispatch of the shell protocol: binding, requests and cleanup.

use super::ShellClient;
use super::minimize::MinimizeTarget;
use super::trust::client_may_bind;
use crate::state::DesiliumState;
use crate::state::layers::SurfaceEffects;
use desilium_protocols::server::desilium_shell_manager_v1::{
    DesiliumShellManagerV1, Error, Request as ManagerRequest,
};
use desilium_protocols::server::desilium_toplevel_handle_v1::{
    DesiliumToplevelHandleV1, Request as HandleRequest,
};
use desilium_wm::{Rect, WindowId};
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::wlr_layer::LayerSurfaceData;
use std::collections::HashMap;

/// User data of a toplevel handle: the window it stands for.
pub struct HandleData {
    pub id: WindowId,
}

impl GlobalDispatch<DesiliumShellManagerV1, ()> for DesiliumState {
    fn bind(
        state: &mut Self,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<DesiliumShellManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        state.shell.clients.push(ShellClient {
            manager,
            last: None,
            handles: HashMap::new(),
        });
        state.shell_sync();
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        client_may_bind(&client)
    }
}

impl Dispatch<DesiliumShellManagerV1, ()> for DesiliumState {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &DesiliumShellManagerV1,
        request: ManagerRequest,
        _data: &(),
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ManagerRequest::Destroy => {}
            // Requests of later protocol versions are ignored.
            ManagerRequest::SwitchWorkspace { index } => {
                state.switch_desktop(to_index(index));
            }
            ManagerRequest::SetSurfaceEffects {
                surface,
                blur_radius,
                radius_top_left,
                radius_top_right,
                radius_bottom_right,
                radius_bottom_left,
            } => {
                if !is_layer_surface(&surface) {
                    resource.post_error(Error::InvalidSurface, "not a layer surface");
                    return;
                }
                let radii = [
                    radius_top_left,
                    radius_top_right,
                    radius_bottom_right,
                    radius_bottom_left,
                ];
                let effects = SurfaceEffects::from_request(blur_radius, radii);
                state.shell.effects.set(&surface, effects);
                state.request_redraw();
            }
            _ => {}
        }
    }

    fn destroyed(
        state: &mut Self,
        _client: ClientId,
        resource: &DesiliumShellManagerV1,
        _data: &(),
    ) {
        state
            .shell
            .clients
            .retain(|client| client.manager.id() != resource.id());
    }
}

impl Dispatch<DesiliumToplevelHandleV1, HandleData> for DesiliumState {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &DesiliumToplevelHandleV1,
        request: HandleRequest,
        data: &HandleData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let id = data.id;
        match request {
            HandleRequest::Destroy => {}
            HandleRequest::Activate => state.shell_activate_window(id),
            HandleRequest::Close => state.request_close_animated(id),
            HandleRequest::SetMinimized { minimized } => {
                state.shell_set_minimized(id, minimized != 0);
            }
            HandleRequest::SetMaximized { maximized } => {
                state.shell_set_maximized(id, maximized != 0);
            }
            HandleRequest::MoveToWorkspace { index } => {
                state.shell_move_to_workspace(id, to_index(index));
            }
            HandleRequest::SetMinimizeRectangle {
                surface,
                x,
                y,
                width,
                height,
            } => {
                let rect = Rect::new(x, y, width, height);
                set_minimize_rectangle(state, resource, id, surface, rect);
            }
            _ => {}
        }
    }

    fn destroyed(
        state: &mut Self,
        _client: ClientId,
        resource: &DesiliumToplevelHandleV1,
        data: &HandleData,
    ) {
        for client in &mut state.shell.clients {
            if client
                .handles
                .get(&data.id)
                .is_some_and(|h| Resource::id(h) == resource.id())
            {
                client.handles.remove(&data.id);
            }
        }
    }
}

fn set_minimize_rectangle(
    state: &mut DesiliumState,
    handle: &DesiliumToplevelHandleV1,
    id: WindowId,
    surface: Option<WlSurface>,
    rect: Rect,
) {
    let Some(surface) = surface else {
        state.store_minimize_target(id, None);
        return;
    };
    if !is_layer_surface(&surface) {
        handle.post_error(Error::InvalidSurface, "not a layer surface");
        return;
    }
    if rect.size.w < 0 || rect.size.h < 0 {
        handle.post_error(Error::InvalidRectangle, "negative size");
        return;
    }
    state.store_minimize_target(id, Some(MinimizeTarget { surface, rect }));
}

fn is_layer_surface(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states.data_map.get::<LayerSurfaceData>().is_some()
    })
}

fn to_index(index: u32) -> usize {
    usize::try_from(index).unwrap_or(usize::MAX)
}
