//! Client of `desilium_shell_manager_v1`: window list, workspaces, output and shortcuts.
//!
//! The objects live on their own event queue of the connection the UI host uses.
//! The host reads the socket and the queue is drained right after, so the shell
//! needs one poll fd for everything.

use desilium_protocols::client::{
    desilium_shell_manager_v1::{self, DesiliumShellManagerV1},
    desilium_toplevel_handle_v1::{self, DesiliumToplevelHandleV1},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_surface::WlSurface},
};

use crate::model::windows::{WindowId, WindowInfo, Workspaces};

/// `desilium_toplevel_handle_v1.state` values.
const STATE_ACTIVATED: u32 = 0;
const STATE_MINIMIZED: u32 = 1;
const BYTES_PER_STATE: usize = 4;
/// Fractional scale denominator of `output_geometry`.
const SCALE_DENOMINATOR: f64 = 120.0;

/// Why the protocol could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// The registry could not be read.
    #[error("cannot read the Wayland registry: {0}")]
    Registry(String),
    /// A roundtrip or dispatch failed (usually: the compositor went away).
    #[error("shell protocol dispatch failed: {0}")]
    Dispatch(String),
}

/// Output size and scale as reported by the compositor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputInfo {
    /// Logical width.
    pub width: i32,
    /// Logical height.
    pub height: i32,
    /// Scale factor.
    pub scale: f64,
}

/// Changes since the last [`ShellProtocol::take_update`].
#[derive(Debug, Clone, Default)]
pub struct Update {
    /// The full window list in creation order, when it changed.
    pub windows: Option<Vec<WindowInfo>>,
    /// New workspace state.
    pub workspaces: Option<Workspaces>,
    /// New output geometry.
    pub output: Option<OutputInfo>,
    /// Shortcut names the compositor sent, in order.
    pub shortcuts: Vec<String>,
}

struct Tracked {
    handle: DesiliumToplevelHandleV1,
    info: WindowInfo,
    closed: bool,
}

#[derive(Default)]
struct ProtoState {
    tracked: Vec<Tracked>,
    windows_dirty: bool,
    workspaces: Option<Workspaces>,
    output: Option<OutputInfo>,
    shortcuts: Vec<String>,
}

impl ProtoState {
    fn tracked_mut(&mut self, handle: &DesiliumToplevelHandleV1) -> Option<&mut Tracked> {
        self.tracked.iter_mut().find(|t| &t.handle == handle)
    }

    fn finish_batch(&mut self) {
        let before = self.tracked.len();
        self.tracked.retain(|t| {
            if t.closed {
                t.handle.destroy();
            }
            !t.closed
        });
        self.windows_dirty |= before != self.tracked.len();
    }
}

fn parse_states(bytes: &[u8], info: &mut WindowInfo) {
    let values: Vec<u32> = bytes
        .as_chunks::<BYTES_PER_STATE>()
        .0
        .iter()
        .map(|chunk| u32::from_ne_bytes(*chunk))
        .collect();
    info.activated = values.contains(&STATE_ACTIVATED);
    info.minimized = values.contains(&STATE_MINIMIZED);
}

impl Dispatch<WlRegistry, GlobalListContents> for ProtoState {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<DesiliumShellManagerV1, ()> for ProtoState {
    fn event(
        state: &mut Self,
        _: &DesiliumShellManagerV1,
        event: desilium_shell_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use desilium_shell_manager_v1::Event;
        match event {
            Event::Toplevel { handle } => {
                state.tracked.push(Tracked {
                    handle,
                    info: WindowInfo::new(WindowId(0)),
                    closed: false,
                });
                state.windows_dirty = true;
            }
            Event::Workspaces { count, active } => {
                let count = count.max(1);
                state.workspaces = Some(Workspaces {
                    count,
                    active: active.min(count - 1),
                });
            }
            Event::OutputGeometry {
                width,
                height,
                scale_120ths,
                ..
            } => {
                state.output = Some(OutputInfo {
                    width: i32::try_from(width).unwrap_or(i32::MAX),
                    height: i32::try_from(height).unwrap_or(i32::MAX),
                    scale: f64::from(scale_120ths) / SCALE_DENOMINATOR,
                });
            }
            Event::Shortcut { name } => state.shortcuts.push(name),
            Event::Done => state.finish_batch(),
            _ => {}
        }
    }

    wayland_client::event_created_child!(ProtoState, DesiliumShellManagerV1, [
        desilium_shell_manager_v1::EVT_TOPLEVEL_OPCODE => (DesiliumToplevelHandleV1, ()),
    ]);
}

impl Dispatch<DesiliumToplevelHandleV1, ()> for ProtoState {
    fn event(
        state: &mut Self,
        handle: &DesiliumToplevelHandleV1,
        event: desilium_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use desilium_toplevel_handle_v1::Event;
        let Some(tracked) = state.tracked_mut(handle) else {
            return;
        };
        match event {
            Event::WindowId { id } => tracked.info.id = WindowId(id),
            Event::Title { title } => tracked.info.title = title,
            Event::AppId { app_id } => tracked.info.app_id = app_id,
            Event::State { state: bytes } => parse_states(&bytes, &mut tracked.info),
            Event::Workspace { index } => tracked.info.workspace = index,
            Event::Geometry {
                x,
                y,
                width,
                height,
            } => {
                let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
                tracked.info.rect = (x, y, clamp(width), clamp(height));
            }
            Event::Closed => tracked.closed = true,
            Event::Done => {}
            _ => return,
        }
        state.windows_dirty = true;
    }
}

/// The bound manager and everything it reported.
pub struct ShellProtocol {
    queue: EventQueue<ProtoState>,
    state: ProtoState,
    manager: DesiliumShellManagerV1,
}

impl ShellProtocol {
    /// Binds the manager on a new queue of `connection`.
    ///
    /// Returns `Ok(None)` when the compositor does not offer it (or does not trust us):
    /// the shell then runs without windows and workspaces.
    pub fn connect(connection: &Connection) -> Result<Option<Self>, ProtocolError> {
        let (globals, mut queue) = registry_queue_init::<ProtoState>(connection)
            .map_err(|error| ProtocolError::Registry(error.to_string()))?;
        let qh = queue.handle();
        let Ok(manager) = globals.bind::<DesiliumShellManagerV1, _, _>(&qh, 1..=1, ()) else {
            return Ok(None);
        };
        let mut state = ProtoState::default();
        queue
            .roundtrip(&mut state)
            .map_err(|error| ProtocolError::Dispatch(error.to_string()))?;
        Ok(Some(Self {
            queue,
            state,
            manager,
        }))
    }

    /// Handles events the shared socket reader already queued for this protocol.
    pub fn dispatch_pending(&mut self) -> Result<(), ProtocolError> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|error| ProtocolError::Dispatch(error.to_string()))
    }

    /// Takes the changes accumulated by [`dispatch_pending`](Self::dispatch_pending).
    pub fn take_update(&mut self) -> Update {
        let windows = std::mem::take(&mut self.state.windows_dirty)
            .then(|| self.state.tracked.iter().map(|t| t.info.clone()).collect());
        Update {
            windows,
            workspaces: self.state.workspaces.take(),
            output: self.state.output.take(),
            shortcuts: std::mem::take(&mut self.state.shortcuts),
        }
    }

    fn handle(&self, id: WindowId) -> Option<&DesiliumToplevelHandleV1> {
        self.state
            .tracked
            .iter()
            .find(|t| t.info.id == id)
            .map(|t| &t.handle)
    }

    /// Focuses and raises a window (restores it when minimized).
    pub fn activate(&self, id: WindowId) {
        if let Some(handle) = self.handle(id) {
            handle.activate();
        }
    }

    pub fn close_window(&self, id: WindowId) {
        if let Some(handle) = self.handle(id) {
            handle.close();
        }
    }

    /// Minimizes or restores a window.
    pub fn set_minimized(&self, id: WindowId, minimized: bool) {
        if let Some(handle) = self.handle(id) {
            handle.set_minimized(u32::from(minimized));
        }
    }

    /// Moves a window to a workspace (zero-based); the compositor ignores bad indices.
    pub fn move_to_workspace(&self, id: WindowId, index: u32) {
        if let Some(handle) = self.handle(id) {
            handle.move_to_workspace(index);
        }
    }

    /// Places a window at a global rectangle `(x, y, width, height)`.
    pub fn set_geometry(&self, id: WindowId, rect: (i32, i32, i32, i32)) {
        let (x, y, width, height) = rect;
        if let Some(handle) = self.handle(id) {
            handle.set_geometry(x, y, width.max(1) as u32, height.max(1) as u32);
        }
    }

    /// Switches to a workspace (zero-based).
    pub fn switch_workspace(&self, index: u32) {
        self.manager.switch_workspace(index);
    }

    /// Asks for a backdrop blur and rounded corners behind a layer surface.
    pub fn set_surface_effects(&self, surface: &WlSurface, blur: u32, radii: [u32; 4]) {
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        self.manager.set_surface_effects(
            surface,
            blur,
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        );
    }

    /// Tells the compositor where a window's dock icon is (surface-local logical pixels).
    pub fn set_minimize_rectangle(
        &self,
        id: WindowId,
        surface: Option<&WlSurface>,
        rect: (i32, i32, i32, i32),
    ) {
        let Some(handle) = self.handle(id) else {
            return;
        };
        let (x, y, width, height) = rect;
        handle.set_minimize_rectangle(surface, x, y, width.max(0), height.max(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_array_is_decoded() {
        let mut info = WindowInfo::new(WindowId(1));
        let bytes: Vec<u8> = [STATE_ACTIVATED, 2, 7]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        parse_states(&bytes, &mut info);
        assert!(info.activated);
        assert!(!info.minimized);
        parse_states(&STATE_MINIMIZED.to_ne_bytes(), &mut info);
        assert!(!info.activated && info.minimized);
        parse_states(&[1, 2, 3], &mut info);
        assert!(
            !info.activated && !info.minimized,
            "truncated array is empty"
        );
    }
}
