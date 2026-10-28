//! Client of KWin's window, virtual desktop and blur protocols.
//!
//! - `org_kde_plasma_window_management`: the window list and window actions,
//! - `org_kde_plasma_virtual_desktop_management`: desktops and switching,
//! - `org_kde_kwin_blur_manager`: backdrop blur behind the shell's own surfaces.
//!
//! KWin hands the first two only to clients it trusts (`X-KDE-Wayland-Interfaces` in the
//! shell's desktop file); when they are refused the shell runs without windows and desktops.
//! The objects live on their own event queue of the connection the UI host uses. The host
//! reads the socket and the queue is drained right after, so the shell needs one poll fd.

mod blur;
mod state;

use std::{cell::RefCell, collections::HashMap};

use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::{wl_compositor::WlCompositor, wl_surface::WlSurface},
};
use wayland_protocols_plasma::{
    blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
    plasma_virtual_desktop::client::org_kde_plasma_virtual_desktop_management::OrgKdePlasmaVirtualDesktopManagement,
    plasma_window_management::client::org_kde_plasma_window_management::OrgKdePlasmaWindowManagement,
};

use crate::model::windows::{WindowId, WindowInfo, Workspaces};
use blur::{BlurEntry, Effect, blur_rects};
use state::{ProtoState, STATE_ACTIVE, STATE_MINIMIZED, Tracked};

/// Oldest `org_kde_plasma_window_management` with `window_with_uuid` and `get_window_by_uuid`.
const WINDOW_MANAGEMENT_VERSIONS: std::ops::RangeInclusive<u32> = 13..=18;
const DESKTOP_MANAGEMENT_VERSIONS: std::ops::RangeInclusive<u32> = 1..=2;
const BLUR_VERSIONS: std::ops::RangeInclusive<u32> = 1..=1;
const COMPOSITOR_VERSIONS: std::ops::RangeInclusive<u32> = 1..=4;
/// Round trips after binding: windows, then their state, then desktop objects.
const SETTLE_ROUNDTRIPS: usize = 3;

/// Why the protocol could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// The registry could not be read.
    #[error("cannot read the Wayland registry: {0}")]
    Registry(String),
    /// A roundtrip or dispatch failed (usually: the compositor went away).
    #[error("KDE protocol dispatch failed: {0}")]
    Dispatch(String),
}

/// Changes since the last [`ShellProtocol::take_update`].
#[derive(Debug, Clone, Default)]
pub struct Update {
    /// The full window list in creation order, when it changed.
    pub windows: Option<Vec<WindowInfo>>,
    /// New workspace state.
    pub workspaces: Option<Workspaces>,
}

/// The bound protocols and everything they reported.
pub struct ShellProtocol {
    queue: EventQueue<ProtoState>,
    state: ProtoState,
    handle: QueueHandle<ProtoState>,
    windows: Option<OrgKdePlasmaWindowManagement>,
    /// Held so the manager object lives as long as the desktops it announced.
    _desktops: Option<OrgKdePlasmaVirtualDesktopManagement>,
    blur_manager: Option<OrgKdeKwinBlurManager>,
    compositor: Option<WlCompositor>,
    blurred: RefCell<Vec<BlurEntry>>,
    minimize_panels: RefCell<HashMap<WindowId, WlSurface>>,
}

impl ShellProtocol {
    /// Binds what KWin offers on a new queue of `connection`.
    ///
    /// Returns `Ok(None)` when nothing usable is offered: the shell then runs without windows,
    /// workspaces and effects.
    pub fn connect(connection: &Connection) -> Result<Option<Self>, ProtocolError> {
        let (globals, mut queue) = registry_queue_init::<ProtoState>(connection)
            .map_err(|error| ProtocolError::Registry(error.to_string()))?;
        let handle = queue.handle();
        let windows = bind(&globals, &handle, WINDOW_MANAGEMENT_VERSIONS);
        let desktops = bind(&globals, &handle, DESKTOP_MANAGEMENT_VERSIONS);
        let blur_manager = bind(&globals, &handle, BLUR_VERSIONS);
        let compositor = bind(&globals, &handle, COMPOSITOR_VERSIONS);
        if windows.is_none() {
            tracing::warn!(
                "org_kde_plasma_window_management is not offered to the shell: no window list. \
                 KWin only grants it to executables named in an X-KDE-Wayland-Interfaces desktop file"
            );
        }
        if desktops.is_none() {
            tracing::warn!(
                "org_kde_plasma_virtual_desktop_management is not offered: one workspace"
            );
        }
        if blur_manager.is_none() {
            tracing::info!("org_kde_kwin_blur_manager is not offered: panels are not blurred");
        }
        if windows.is_none() && desktops.is_none() && blur_manager.is_none() {
            return Ok(None);
        }
        let mut state = ProtoState::default();
        for _ in 0..SETTLE_ROUNDTRIPS {
            queue
                .roundtrip(&mut state)
                .map_err(|error| ProtocolError::Dispatch(error.to_string()))?;
        }
        Ok(Some(Self {
            queue,
            state,
            handle,
            windows,
            _desktops: desktops,
            blur_manager,
            compositor,
            blurred: RefCell::new(Vec::new()),
            minimize_panels: RefCell::new(HashMap::new()),
        }))
    }

    /// KWin gave this shell the window list, so windows can be listed and controlled.
    pub fn has_windows(&self) -> bool {
        self.windows.is_some()
    }

    /// Handles events the shared socket reader already queued for these protocols.
    pub fn dispatch_pending(&mut self) -> Result<(), ProtocolError> {
        self.queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|error| ProtocolError::Dispatch(error.to_string()))
    }

    /// Takes the changes accumulated by [`dispatch_pending`](Self::dispatch_pending).
    pub fn take_update(&mut self) -> Update {
        let removed = self.state.forget_closed();
        let windows_dirty = std::mem::take(&mut self.state.windows_dirty) || removed;
        let workspaces_dirty = std::mem::take(&mut self.state.workspaces_dirty);
        Update {
            windows: windows_dirty.then(|| self.state.window_list()),
            workspaces: workspaces_dirty.then(|| self.state.workspaces()),
        }
    }

    fn tracked(&self, id: WindowId) -> Option<&Tracked> {
        self.state
            .tracked
            .iter()
            .find(|tracked| tracked.record.info.id == id)
    }

    /// Focuses and raises a window (restores it when minimized).
    pub fn activate(&self, id: WindowId) {
        if let Some(tracked) = self.tracked(id) {
            tracked
                .handle
                .set_state(STATE_ACTIVE | STATE_MINIMIZED, STATE_ACTIVE);
        }
    }

    /// Asks a window to close.
    pub fn close_window(&self, id: WindowId) {
        if let Some(tracked) = self.tracked(id) {
            tracked.handle.close();
        }
    }

    /// Minimizes or restores a window.
    pub fn set_minimized(&self, id: WindowId, minimized: bool) {
        if let Some(tracked) = self.tracked(id) {
            let state = if minimized { STATE_MINIMIZED } else { 0 };
            tracked.handle.set_state(STATE_MINIMIZED, state);
        }
    }

    /// Moves a window to a workspace (zero-based); a bad index is ignored.
    ///
    /// The window enters the new desktop first and leaves the others afterwards: leaving the
    /// last one would make it appear on all desktops.
    pub fn move_to_workspace(&self, id: WindowId, index: u32) {
        let Some(tracked) = self.tracked(id) else {
            return;
        };
        let Some(target) = usize::try_from(index)
            .ok()
            .and_then(|index| self.state.desktops.get(index))
        else {
            return;
        };
        tracked
            .handle
            .request_enter_virtual_desktop(target.id.clone());
        for other in tracked
            .record
            .desktops
            .iter()
            .filter(|desktop| **desktop != target.id)
        {
            tracked.handle.request_leave_virtual_desktop(other.clone());
        }
    }

    /// Places a window at a global rectangle `(x, y, width, height)`.
    ///
    /// The protocol cannot do this; a one-shot KWin script does, on a helper thread because it
    /// talks to D-Bus and the disk.
    pub fn set_geometry(&self, id: WindowId, rect: (i32, i32, i32, i32)) {
        let Some(tracked) = self.tracked(id) else {
            return;
        };
        let uuid = tracked.record.uuid.clone();
        let spawned = std::thread::Builder::new()
            .name("desilium-geometry".to_owned())
            .spawn(move || {
                if let Err(error) = desilium_kwin::windows::set_geometry(&uuid, rect) {
                    tracing::warn!(%error, "window could not be placed");
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "cannot start the placement thread");
        }
    }

    /// Switches to a workspace (zero-based).
    pub fn switch_workspace(&self, index: u32) {
        let desktop = usize::try_from(index)
            .ok()
            .and_then(|index| self.state.desktops.get(index));
        if let Some(desktop) = desktop {
            desktop.handle.request_activate();
        }
    }

    /// Asks KWin to blur what is behind `surface`, with rounded corners `radii`.
    ///
    /// `size` is the surface's logical size. A `blur` of zero removes the effect. Returns whether
    /// pending state was changed; the caller then commits the surface so KWin applies it.
    pub fn set_surface_effects(
        &self,
        surface: &WlSurface,
        size: (u32, u32),
        blur: u32,
        radii: [u32; 4],
    ) -> bool {
        let Some(manager) = &self.blur_manager else {
            return false;
        };
        let mut entries = self.blurred.borrow_mut();
        entries.retain(BlurEntry::alive);
        let wanted = Effect { size, blur, radii };
        let position = entries.iter().position(|e| e.surface.id() == surface.id());
        if blur == 0 {
            let Some(position) = position else {
                return false;
            };
            let entry = entries.swap_remove(position);
            manager.unset(&entry.surface);
            entry.blur.release();
            return true;
        }
        if position.is_some_and(|at| entries[at].applied == wanted) {
            return false;
        }
        let at = position.unwrap_or_else(|| {
            let object = manager.create(surface, &self.handle, ());
            entries.push(BlurEntry {
                surface: surface.clone(),
                blur: object,
                applied: Effect {
                    size: (0, 0),
                    blur: 0,
                    radii: [0; 4],
                },
            });
            entries.len() - 1
        });
        let entry = &mut entries[at];
        self.set_region(entry, &wanted);
        entry.blur.commit();
        entry.applied = wanted;
        true
    }

    fn set_region(&self, entry: &BlurEntry, effect: &Effect) {
        let Some(compositor) = &self.compositor else {
            // Without a region KWin blurs the whole surface.
            entry.blur.set_region(None);
            return;
        };
        let region = compositor.create_region(&self.handle, ());
        for (x, y, width, height) in blur_rects(effect.size, effect.radii) {
            region.add(x, y, width, height);
        }
        entry.blur.set_region(Some(&region));
        region.destroy();
    }

    /// Tells KWin where a window's dock icon is (surface-local logical pixels), for its
    /// minimize animation. `None` forgets the target.
    pub fn set_minimize_rectangle(
        &self,
        id: WindowId,
        surface: Option<&WlSurface>,
        rect: (i32, i32, i32, i32),
    ) {
        let Some(tracked) = self.tracked(id) else {
            return;
        };
        let mut panels = self.minimize_panels.borrow_mut();
        let Some(surface) = surface else {
            if let Some(panel) = panels.remove(&id) {
                tracked.handle.unset_minimized_geometry(&panel);
            }
            return;
        };
        let (x, y, width, height) = rect;
        let clamp = |value: i32| u32::try_from(value.max(0)).unwrap_or(0);
        tracked.handle.set_minimized_geometry(
            surface,
            clamp(x),
            clamp(y),
            clamp(width),
            clamp(height),
        );
        panels.insert(id, surface.clone());
    }
}

/// Binds a global of a version in `versions`, or `None` when it is absent or refused.
fn bind<I>(
    globals: &GlobalList,
    handle: &QueueHandle<ProtoState>,
    versions: std::ops::RangeInclusive<u32>,
) -> Option<I>
where
    I: Proxy + 'static,
    ProtoState: wayland_client::Dispatch<I, ()>,
{
    globals.bind::<I, _, _>(handle, versions, ()).ok()
}
