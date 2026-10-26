//! Turns two snapshots into the minimal list of events a shell client needs.

use super::snapshot::{OutputInfo, ShellSnapshot, WindowFlags, WindowInfo, WorkspaceInfo};
use desilium_wm::WindowId;
use std::collections::HashMap;

/// Changed properties of a window the shell already knows; unchanged ones are `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowChange {
    pub id: WindowId,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub flags: Option<WindowFlags>,
    pub workspace: Option<usize>,
}

/// One step of a shell update. The sender ends a non-empty batch with `done`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    WindowClosed(WindowId),
    WindowAdded(WindowInfo),
    WindowChanged(WindowChange),
    Workspaces(WorkspaceInfo),
    Output(OutputInfo),
}

/// Events that bring a client from `prev` to `next`; everything when `prev` is `None`.
///
/// Order: closed windows, new windows, changed windows, workspaces, output. Windows keep
/// the snapshot order (creation order), so a fresh bind sees the windows oldest first.
pub fn diff(prev: Option<&ShellSnapshot>, next: &ShellSnapshot) -> Vec<ShellEvent> {
    let mut events = Vec::new();
    let known: HashMap<WindowId, &WindowInfo> = prev
        .map(|snapshot| snapshot.windows.iter().map(|w| (w.id, w)).collect())
        .unwrap_or_default();
    push_closed(&mut events, prev, next);
    for window in &next.windows {
        match known.get(&window.id) {
            None => events.push(ShellEvent::WindowAdded(window.clone())),
            Some(old) => events.extend(window_change(old, window).map(ShellEvent::WindowChanged)),
        }
    }
    if prev.is_none_or(|snapshot| snapshot.workspaces != next.workspaces) {
        events.push(ShellEvent::Workspaces(next.workspaces));
    }
    let old_output = prev.and_then(|snapshot| snapshot.output);
    if let Some(output) = next.output.filter(|output| Some(*output) != old_output) {
        events.push(ShellEvent::Output(output));
    }
    events
}

fn push_closed(events: &mut Vec<ShellEvent>, prev: Option<&ShellSnapshot>, next: &ShellSnapshot) {
    let Some(prev) = prev else {
        return;
    };
    let alive: std::collections::HashSet<WindowId> = next.windows.iter().map(|w| w.id).collect();
    let closed = prev.windows.iter().filter(|w| !alive.contains(&w.id));
    events.extend(closed.map(|window| ShellEvent::WindowClosed(window.id)));
}

fn window_change(old: &WindowInfo, new: &WindowInfo) -> Option<WindowChange> {
    let change = WindowChange {
        id: new.id,
        title: (old.title != new.title).then(|| new.title.clone()),
        app_id: (old.app_id != new.app_id).then(|| new.app_id.clone()),
        flags: (old.flags != new.flags).then_some(new.flags),
        workspace: (old.workspace != new.workspace).then_some(new.workspace),
    };
    let unchanged = change.title.is_none()
        && change.app_id.is_none()
        && change.flags.is_none()
        && change.workspace.is_none();
    (!unchanged).then_some(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_wm::{Desktops, Rect, Size};

    fn ids(count: usize) -> Vec<WindowId> {
        let mut desks = Desktops::new(Rect::new(0, 0, 800, 600), 1);
        (0..count)
            .map(|_| desks.open_window(Size { w: 10, h: 10 }))
            .collect()
    }

    fn window(id: WindowId, title: &str) -> WindowInfo {
        WindowInfo {
            id,
            title: title.to_owned(),
            app_id: "app".to_owned(),
            flags: WindowFlags::default(),
            workspace: 0,
        }
    }

    fn output(width: i32) -> OutputInfo {
        OutputInfo {
            area: Rect::new(0, 0, width, 600),
            work_area: Rect::new(0, 30, width, 570),
            scale_120ths: 120,
        }
    }

    fn snapshot(windows: Vec<WindowInfo>) -> ShellSnapshot {
        ShellSnapshot::new(
            windows,
            WorkspaceInfo {
                count: 4,
                active: 0,
            },
            Some(output(800)),
        )
    }

    #[test]
    fn first_snapshot_sends_everything() {
        let ids = ids(2);
        let next = snapshot(vec![window(ids[1], "b"), window(ids[0], "a")]);
        let events = diff(None, &next);
        assert_eq!(
            events,
            vec![
                ShellEvent::WindowAdded(window(ids[0], "a")),
                ShellEvent::WindowAdded(window(ids[1], "b")),
                ShellEvent::Workspaces(next.workspaces),
                ShellEvent::Output(output(800)),
            ]
        );
    }

    #[test]
    fn first_snapshot_without_output_skips_it() {
        let next = ShellSnapshot::new(
            vec![],
            WorkspaceInfo {
                count: 1,
                active: 0,
            },
            None,
        );
        assert_eq!(diff(None, &next).len(), 1);
    }

    #[test]
    fn identical_snapshots_produce_nothing() {
        let ids = ids(1);
        let snap = snapshot(vec![window(ids[0], "a")]);
        assert!(diff(Some(&snap), &snap.clone()).is_empty());
    }

    #[test]
    fn title_change_only_carries_the_title() {
        let ids = ids(1);
        let old = snapshot(vec![window(ids[0], "a")]);
        let new = snapshot(vec![window(ids[0], "b")]);
        assert_eq!(
            diff(Some(&old), &new),
            vec![ShellEvent::WindowChanged(WindowChange {
                id: ids[0],
                title: Some("b".to_owned()),
                app_id: None,
                flags: None,
                workspace: None,
            })]
        );
    }

    #[test]
    fn several_property_changes_share_one_event() {
        let ids = ids(1);
        let old = snapshot(vec![window(ids[0], "a")]);
        let mut changed = window(ids[0], "a");
        changed.flags.activated = true;
        changed.workspace = 2;
        changed.app_id = "other".to_owned();
        let events = diff(Some(&old), &snapshot(vec![changed.clone()]));
        assert_eq!(
            events,
            vec![ShellEvent::WindowChanged(WindowChange {
                id: ids[0],
                title: None,
                app_id: Some("other".to_owned()),
                flags: Some(changed.flags),
                workspace: Some(2),
            })]
        );
    }

    #[test]
    fn closed_windows_come_before_new_ones() {
        let ids = ids(3);
        let old = snapshot(vec![window(ids[0], "a"), window(ids[1], "b")]);
        let new = snapshot(vec![window(ids[1], "b"), window(ids[2], "c")]);
        assert_eq!(
            diff(Some(&old), &new),
            vec![
                ShellEvent::WindowClosed(ids[0]),
                ShellEvent::WindowAdded(window(ids[2], "c")),
            ]
        );
    }

    #[test]
    fn workspace_change_is_reported_alone() {
        let old = snapshot(vec![]);
        let mut new = old.clone();
        new.workspaces.active = 3;
        assert_eq!(
            diff(Some(&old), &new),
            vec![ShellEvent::Workspaces(new.workspaces)]
        );
    }

    #[test]
    fn output_change_is_reported_alone() {
        let old = snapshot(vec![]);
        let mut new = old.clone();
        new.output = Some(output(1024));
        assert_eq!(
            diff(Some(&old), &new),
            vec![ShellEvent::Output(output(1024))]
        );
    }

    #[test]
    fn output_disappearing_sends_nothing() {
        let old = snapshot(vec![]);
        let mut new = old.clone();
        new.output = None;
        assert!(diff(Some(&old), &new).is_empty());
    }

    #[test]
    fn replaying_diffs_converges_to_the_last_snapshot() {
        let ids = ids(3);
        let steps = [
            snapshot(vec![window(ids[0], "a")]),
            snapshot(vec![window(ids[0], "a2"), window(ids[1], "b")]),
            snapshot(vec![window(ids[1], "b")]),
            snapshot(vec![window(ids[1], "b"), window(ids[2], "c")]),
        ];
        let mut prev: Option<&ShellSnapshot> = None;
        for step in &steps {
            let events = diff(prev, step);
            assert_eq!(events.is_empty(), prev == Some(step));
            prev = Some(step);
        }
        assert!(diff(prev, steps.last().unwrap_or(&steps[0])).is_empty());
    }
}
