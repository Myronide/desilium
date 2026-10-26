//! Writes diff events to a client as protocol events.

use super::diff::{ShellEvent, WindowChange, diff};
use super::handlers::HandleData;
use super::snapshot::{OutputInfo, ShellSnapshot, WindowFlags, WindowInfo};
use super::{PROTOCOL_VERSION, ShellClient};
use crate::state::DesiliumState;
use desilium_protocols::server::desilium_toplevel_handle_v1::{DesiliumToplevelHandleV1, State};
use smithay::reexports::wayland_server::{DisplayHandle, Resource};
use tracing::warn;

/// Brings one client up to date with `snapshot`; the batch ends with `done`.
pub(super) fn publish(dh: &DisplayHandle, client: &mut ShellClient, snapshot: &ShellSnapshot) {
    if !client.manager.is_alive() {
        return;
    }
    let events = diff(client.last.as_ref(), snapshot);
    client.last = Some(snapshot.clone());
    if events.is_empty() {
        return;
    }
    for event in &events {
        send_event(dh, client, event);
    }
    client.manager.done();
}

fn send_event(dh: &DisplayHandle, client: &mut ShellClient, event: &ShellEvent) {
    match event {
        ShellEvent::WindowClosed(id) => {
            let Some(handle) = client.handles.remove(id) else {
                return;
            };
            handle.closed();
            handle.done();
        }
        ShellEvent::WindowAdded(info) => send_added(dh, client, info),
        ShellEvent::WindowChanged(change) => send_changed(client, change),
        ShellEvent::Workspaces(info) => client
            .manager
            .workspaces(wire_index(info.count), wire_index(info.active)),
        ShellEvent::Output(info) => send_output(client, info),
    }
}

fn send_added(dh: &DisplayHandle, client: &mut ShellClient, info: &WindowInfo) {
    let Some(owner) = client.manager.client() else {
        return;
    };
    let data = HandleData { id: info.id };
    let created = owner.create_resource::<DesiliumToplevelHandleV1, HandleData, DesiliumState>(
        dh,
        PROTOCOL_VERSION,
        data,
    );
    let handle = match created {
        Ok(handle) => handle,
        Err(error) => {
            warn!(%error, "creating a toplevel handle failed");
            return;
        }
    };
    client.manager.toplevel(&handle);
    handle.window_id(window_wire_id(info.id.get()));
    handle.title(info.title.clone());
    handle.app_id(info.app_id.clone());
    handle.state(state_array(info.flags));
    handle.workspace(wire_index(info.workspace));
    handle.done();
    client.handles.insert(info.id, handle);
}

fn send_changed(client: &ShellClient, change: &WindowChange) {
    // The shell may have destroyed the handle already; that is its choice.
    let Some(handle) = client.handles.get(&change.id) else {
        return;
    };
    if let Some(title) = &change.title {
        handle.title(title.clone());
    }
    if let Some(app_id) = &change.app_id {
        handle.app_id(app_id.clone());
    }
    if let Some(flags) = change.flags {
        handle.state(state_array(flags));
    }
    if let Some(workspace) = change.workspace {
        handle.workspace(wire_index(workspace));
    }
    handle.done();
}

fn send_output(client: &ShellClient, info: &OutputInfo) {
    client.manager.output_geometry(
        info.area.loc.x,
        info.area.loc.y,
        wire_size(info.area.size.w),
        wire_size(info.area.size.h),
        info.work_area.loc.x,
        info.work_area.loc.y,
        wire_size(info.work_area.size.w),
        wire_size(info.work_area.size.h),
        info.scale_120ths,
    );
}

/// Encodes the flags as the native-endian `u32` array of the `state` event.
pub fn state_array(flags: WindowFlags) -> Vec<u8> {
    let states = [
        (flags.activated, State::Activated),
        (flags.minimized, State::Minimized),
        (flags.maximized, State::Maximized),
        (flags.fullscreen, State::Fullscreen),
    ];
    states
        .into_iter()
        .filter(|(on, _)| *on)
        .flat_map(|(_, state)| u32::from(state).to_ne_bytes())
        .collect()
}

/// The 32-bit wire id of a window: ids are sequential, so the low bits are unique in practice.
pub fn window_wire_id(raw: u64) -> u32 {
    u32::try_from(raw & u64::from(u32::MAX)).unwrap_or(u32::MAX)
}

fn wire_index(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

fn wire_size(size: i32) -> u32 {
    u32::try_from(size).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks(4)
            .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok())
            .map(u32::from_ne_bytes)
            .collect()
    }

    #[test]
    fn no_flags_give_an_empty_array() {
        assert!(state_array(WindowFlags::default()).is_empty());
    }

    #[test]
    fn flags_become_enum_values_in_order() {
        let flags = WindowFlags {
            activated: true,
            minimized: false,
            maximized: true,
            fullscreen: true,
        };
        assert_eq!(decode(&state_array(flags)), vec![0, 2, 3]);
    }

    #[test]
    fn wire_id_keeps_small_ids_and_truncates_large_ones() {
        assert_eq!(window_wire_id(7), 7);
        assert_eq!(window_wire_id(u64::from(u32::MAX) + 2), 1);
    }

    #[test]
    fn negative_sizes_and_huge_indices_are_clamped() {
        assert_eq!(wire_size(-4), 0);
        assert_eq!(wire_size(10), 10);
        assert_eq!(wire_index(usize::MAX), u32::MAX);
    }
}
