//! What KWin reported about windows and virtual desktops, and the event handlers that collect it.

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, globals::GlobalListContents,
    protocol::wl_registry::WlRegistry,
};
use wayland_protocols_plasma::{
    plasma_virtual_desktop::client::{
        org_kde_plasma_virtual_desktop::{self, OrgKdePlasmaVirtualDesktop},
        org_kde_plasma_virtual_desktop_management::{self, OrgKdePlasmaVirtualDesktopManagement},
    },
    plasma_window_management::client::{
        org_kde_plasma_window::{self, OrgKdePlasmaWindow},
        org_kde_plasma_window_management::{self, OrgKdePlasmaWindowManagement},
    },
};

use crate::model::windows::{WindowId, WindowInfo, Workspaces};

/// `org_kde_plasma_window_management.state` bits.
pub const STATE_ACTIVE: u32 = 0x1;
pub const STATE_MINIMIZED: u32 = 0x2;
/// Panels, docks and the like: not shown in a task list.
pub const STATE_SKIP_TASKBAR: u32 = 0x1000;

/// Everything known about one window that does not need the Wayland object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowRecord {
    /// Uuid KWin names the window by; the address for scripts.
    pub uuid: String,
    /// Window data in the shell's own model.
    pub info: WindowInfo,
    /// Desktop ids the window is on; empty means on all of them.
    pub desktops: Vec<String>,
    /// Raw state bits.
    pub flags: u32,
    /// `initial_state` has arrived, so the data is complete.
    pub ready: bool,
    /// The window was unmapped.
    pub closed: bool,
}

impl WindowRecord {
    /// A window that was announced and not described yet.
    pub fn new(id: u32, uuid: String) -> Self {
        Self {
            uuid,
            info: WindowInfo::new(WindowId(id)),
            desktops: Vec::new(),
            flags: 0,
            ready: false,
            closed: false,
        }
    }

    /// Takes one event into the record. Returns whether something the shell shows changed.
    pub fn apply(&mut self, event: org_kde_plasma_window::Event) -> bool {
        use org_kde_plasma_window::Event;
        match event {
            Event::TitleChanged { title } => self.info.title = title,
            Event::AppIdChanged { app_id } => self.info.app_id = app_id,
            Event::ThemedIconNameChanged { name } => self.info.icon = name,
            Event::StateChanged { flags } => {
                self.flags = flags;
                self.info.activated = flags & STATE_ACTIVE != 0;
                self.info.minimized = flags & STATE_MINIMIZED != 0;
            }
            Event::VirtualDesktopEntered { id } => {
                if !self.desktops.contains(&id) {
                    self.desktops.push(id);
                }
            }
            Event::VirtualDesktopLeft { is } => self.desktops.retain(|desktop| *desktop != is),
            Event::Geometry {
                x,
                y,
                width,
                height,
            } => {
                let clamp = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
                self.info.rect = (x, y, clamp(width), clamp(height));
            }
            Event::Unmapped => self.closed = true,
            Event::InitialState => self.ready = true,
            _ => return false,
        }
        true
    }

    /// Whether the shell lists the window at all.
    pub fn listed(&self) -> bool {
        self.ready && !self.closed && self.flags & STATE_SKIP_TASKBAR == 0
    }
}

/// Zero-based index of the first of `entered` that is in `order`; the first desktop when the
/// window is on none (all desktops) or only on unknown ones.
pub fn workspace_index(entered: &[String], order: &[&str]) -> u32 {
    entered
        .iter()
        .find_map(|id| order.iter().position(|known| known == id))
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or(0)
}

/// A window with its Wayland object.
pub struct Tracked {
    pub handle: OrgKdePlasmaWindow,
    pub record: WindowRecord,
}

/// A virtual desktop with its Wayland object.
pub struct Desktop {
    pub handle: OrgKdePlasmaVirtualDesktop,
    pub id: String,
    pub active: bool,
}

/// State shared by all handlers of the protocol queue.
#[derive(Default)]
pub struct ProtoState {
    pub tracked: Vec<Tracked>,
    pub desktops: Vec<Desktop>,
    pub windows_dirty: bool,
    pub workspaces_dirty: bool,
}

impl ProtoState {
    /// The windows to show, in creation order, with workspaces resolved.
    pub fn window_list(&self) -> Vec<WindowInfo> {
        let order: Vec<&str> = self.desktops.iter().map(|d| d.id.as_str()).collect();
        self.tracked
            .iter()
            .filter(|tracked| tracked.record.listed())
            .map(|tracked| WindowInfo {
                workspace: workspace_index(&tracked.record.desktops, &order),
                on_all_workspaces: tracked.record.desktops.is_empty(),
                ..tracked.record.info.clone()
            })
            .collect()
    }

    /// Desktop count and the active one.
    pub fn workspaces(&self) -> Workspaces {
        let count = u32::try_from(self.desktops.len())
            .unwrap_or(u32::MAX)
            .max(1);
        let active = self
            .desktops
            .iter()
            .position(|desktop| desktop.active)
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(0);
        Workspaces {
            count,
            active: active.min(count - 1),
        }
    }

    /// Drops windows that were unmapped. Returns true when the visible list may have changed.
    pub fn forget_closed(&mut self) -> bool {
        let before = self.tracked.len();
        self.tracked.retain(|tracked| {
            if tracked.record.closed {
                tracked.handle.destroy();
            }
            !tracked.record.closed
        });
        before != self.tracked.len()
    }
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

impl Dispatch<OrgKdePlasmaWindowManagement, ()> for ProtoState {
    fn event(
        state: &mut Self,
        manager: &OrgKdePlasmaWindowManagement,
        event: org_kde_plasma_window_management::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use org_kde_plasma_window_management::Event;
        // `Window` is the deprecated twin of `WindowWithUuid`; KWin sends both.
        let Event::WindowWithUuid { id, uuid } = event else {
            return;
        };
        if state
            .tracked
            .iter()
            .any(|tracked| tracked.record.uuid == uuid)
        {
            return;
        }
        let handle = manager.get_window_by_uuid(uuid.clone(), qh, ());
        state.tracked.push(Tracked {
            handle,
            record: WindowRecord::new(id, uuid),
        });
    }
}

impl Dispatch<OrgKdePlasmaWindow, ()> for ProtoState {
    fn event(
        state: &mut Self,
        handle: &OrgKdePlasmaWindow,
        event: org_kde_plasma_window::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(tracked) = state.tracked.iter_mut().find(|t| &t.handle == handle) else {
            return;
        };
        if tracked.record.apply(event) {
            state.windows_dirty = true;
        }
    }
}

impl Dispatch<OrgKdePlasmaVirtualDesktopManagement, ()> for ProtoState {
    fn event(
        state: &mut Self,
        manager: &OrgKdePlasmaVirtualDesktopManagement,
        event: org_kde_plasma_virtual_desktop_management::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use org_kde_plasma_virtual_desktop_management::Event;
        match event {
            Event::DesktopCreated {
                desktop_id,
                position,
            } => {
                if state
                    .desktops
                    .iter()
                    .any(|desktop| desktop.id == desktop_id)
                {
                    return;
                }
                let handle = manager.get_virtual_desktop(desktop_id.clone(), qh, ());
                let at = usize::try_from(position)
                    .unwrap_or(usize::MAX)
                    .min(state.desktops.len());
                state.desktops.insert(
                    at,
                    Desktop {
                        handle,
                        id: desktop_id,
                        active: false,
                    },
                );
            }
            Event::DesktopRemoved { desktop_id } => {
                state.desktops.retain(|desktop| desktop.id != desktop_id);
            }
            Event::Done => {}
            _ => return,
        }
        state.workspaces_dirty = true;
        state.windows_dirty = true;
    }
}

impl Dispatch<OrgKdePlasmaVirtualDesktop, ()> for ProtoState {
    fn event(
        state: &mut Self,
        handle: &OrgKdePlasmaVirtualDesktop,
        event: org_kde_plasma_virtual_desktop::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use org_kde_plasma_virtual_desktop::Event;
        match event {
            Event::Activated => set_active(state, handle, true),
            Event::Deactivated => set_active(state, handle, false),
            Event::Removed => state.desktops.retain(|desktop| &desktop.handle != handle),
            _ => return,
        }
        state.workspaces_dirty = true;
        state.windows_dirty = true;
    }
}

fn set_active(state: &mut ProtoState, handle: &OrgKdePlasmaVirtualDesktop, active: bool) {
    if let Some(desktop) = state.desktops.iter_mut().find(|d| &d.handle == handle) {
        desktop.active = active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use org_kde_plasma_window::Event;

    fn record() -> WindowRecord {
        WindowRecord::new(7, "uuid-7".to_owned())
    }

    #[test]
    fn state_bits_become_activated_and_minimized() {
        let mut window = record();
        assert!(window.apply(Event::StateChanged {
            flags: STATE_ACTIVE | 0x100,
        }));
        assert!(window.info.activated && !window.info.minimized);
        window.apply(Event::StateChanged {
            flags: STATE_MINIMIZED,
        });
        assert!(!window.info.activated && window.info.minimized);
        window.apply(Event::StateChanged { flags: 0 });
        assert!(!window.info.activated && !window.info.minimized);
    }

    #[test]
    fn a_window_is_listed_only_after_its_initial_state() {
        let mut window = record();
        assert!(!window.listed());
        window.apply(Event::TitleChanged {
            title: "Terminal".to_owned(),
        });
        assert!(!window.listed());
        window.apply(Event::InitialState);
        assert!(window.listed());
        assert_eq!(window.info.title, "Terminal");
        assert_eq!(window.info.id, WindowId(7));
    }

    #[test]
    fn unmapped_and_taskbar_skipping_windows_are_not_listed() {
        let mut window = record();
        window.apply(Event::InitialState);
        window.apply(Event::StateChanged {
            flags: STATE_SKIP_TASKBAR,
        });
        assert!(!window.listed(), "panels stay out of the task list");
        window.apply(Event::StateChanged { flags: 0 });
        assert!(window.listed());
        window.apply(Event::Unmapped);
        assert!(!window.listed());
    }

    #[test]
    fn desktops_are_entered_and_left_without_duplicates() {
        let mut window = record();
        window.apply(Event::VirtualDesktopEntered { id: "a".to_owned() });
        window.apply(Event::VirtualDesktopEntered { id: "a".to_owned() });
        window.apply(Event::VirtualDesktopEntered { id: "b".to_owned() });
        assert_eq!(window.desktops, ["a", "b"]);
        window.apply(Event::VirtualDesktopLeft { is: "a".to_owned() });
        assert_eq!(window.desktops, ["b"]);
    }

    #[test]
    fn geometry_is_kept_as_signed_position_and_size() {
        let mut window = record();
        window.apply(Event::Geometry {
            x: -10,
            y: 20,
            width: 640,
            height: 480,
        });
        assert_eq!(window.info.rect, (-10, 20, 640, 480));
    }

    #[test]
    fn events_the_shell_does_not_show_report_no_change() {
        let mut window = record();
        assert!(!window.apply(Event::PidChanged { pid: 4 }));
        assert!(!window.apply(Event::IconChanged));
    }

    #[test]
    fn the_icon_name_kwin_reports_is_kept_for_the_dock() {
        let mut window = record();
        assert!(window.apply(Event::ThemedIconNameChanged {
            name: "org.kde.dolphin".to_owned()
        }));
        assert_eq!(window.info.icon, "org.kde.dolphin");
    }

    #[test]
    fn workspace_follows_the_first_known_desktop() {
        let order = ["d0", "d1", "d2"];
        assert_eq!(workspace_index(&[], &order), 0, "on all desktops");
        assert_eq!(workspace_index(&["d2".to_owned()], &order), 2);
        assert_eq!(
            workspace_index(&["gone".to_owned(), "d1".to_owned()], &order),
            1
        );
        assert_eq!(workspace_index(&["gone".to_owned()], &order), 0);
    }
}
