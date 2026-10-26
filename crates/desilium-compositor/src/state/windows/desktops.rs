//! Virtual desktops: showing one desktop at a time and moving windows between them.
//!
//! Only the active desktop's windows are mapped into the render space, so pointer
//! hit-testing and rendering ignore the others. The window manager keeps each desktop's
//! own stacking and focus, so switching back restores them.

use crate::state::DesiliumState;
use desilium_wm::{Desktops, WindowId};
use smithay::input::pointer::MotionEvent;
use smithay::utils::SERIAL_COUNTER;

/// Which way `step_desktop` moves through the desktops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopStep {
    Next,
    Prev,
}

/// Read-only snapshot of the desktops for the shell and the on-screen display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopOverview {
    /// Index of the desktop in view.
    pub active: usize,
    /// Number of desktops.
    pub count: usize,
    /// Window count per desktop, minimized windows included, indexed like the desktops.
    pub windows: Vec<usize>,
}

impl DesiliumState {
    /// Snapshot of the desktops for the shell and the on-screen display.
    #[allow(dead_code)] // consumed by the shell and on-screen display work
    pub fn desktop_overview(&self) -> DesktopOverview {
        overview_of(&self.wm)
    }

    /// Shows desktop `index`. Windows of the other desktops leave the render space.
    pub fn switch_desktop(&mut self, index: usize) {
        if index == self.wm.active() || !self.wm.switch_to(index) {
            return;
        }
        self.hide_windows_off_view();
        self.sync_space();
        self.apply_focus();
        self.refresh_pointer_focus();
    }

    /// Shows the neighbouring desktop in the given direction, wrapping around.
    pub fn step_desktop(&mut self, step: DesktopStep) {
        let target = neighbour_desktop(self.wm.active(), self.wm.count(), step);
        self.switch_desktop(target);
    }

    /// Moves the focused window to desktop `index`. It leaves the view at once; the
    /// current desktop stays in view.
    pub fn move_focused_window_to_desktop(&mut self, index: usize) {
        let Some(id) = self.wm.focused() else {
            return;
        };
        self.move_window_to_desktop(id, index);
    }

    /// Moves window `id` to desktop `index`; the active desktop stays in view.
    pub(crate) fn move_window_to_desktop(&mut self, id: WindowId, index: usize) {
        if index == self.wm.active() || !self.wm.move_window_to(id, index) {
            return;
        }
        if let Some(window) = self.windows.get(&id) {
            self.space.unmap_elem(window);
        }
        self.sync_space();
        self.apply_focus();
        self.refresh_pointer_focus();
    }

    /// Applies a configured desktop count. Windows of removed desktops move to the last one.
    pub fn set_desktop_count(&mut self, count: usize) {
        let before = self.wm.count();
        self.wm.set_count(count);
        if self.wm.count() == before {
            return;
        }
        self.hide_windows_off_view();
        self.sync_space();
        self.apply_focus();
    }

    /// Unmaps every window that is not on the active desktop.
    fn hide_windows_off_view(&mut self) {
        let leaving = windows_leaving_view(&self.wm.all_windows(), self.wm.active());
        for id in leaving {
            if let Some(window) = self.windows.get(&id) {
                self.space.unmap_elem(window);
            }
        }
    }

    /// Re-runs hit-testing at the pointer, because the windows under it just changed.
    fn refresh_pointer_focus(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location = pointer.current_location();
        let under = if self.decoration_covers(location) {
            None
        } else {
            self.surface_under(location)
        };
        let time = u32::try_from(self.start_time.elapsed().as_millis()).unwrap_or(u32::MAX);
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
    }
}

/// Windows that must leave the render space when `active` is shown: all windows of other
/// desktops. `windows` lists `(id, desktop index)` pairs.
pub fn windows_leaving_view(windows: &[(WindowId, usize)], active: usize) -> Vec<WindowId> {
    windows
        .iter()
        .filter(|(_, desktop)| *desktop != active)
        .map(|(id, _)| *id)
        .collect()
}

/// The desktop one step away from `active`, wrapping. A single desktop stays put.
pub fn neighbour_desktop(active: usize, count: usize, step: DesktopStep) -> usize {
    if count == 0 {
        return active;
    }
    match step {
        DesktopStep::Next => active.saturating_add(1) % count,
        DesktopStep::Prev => active.saturating_add(count).saturating_sub(1) % count,
    }
}

/// Builds the overview from the desktops' current state.
pub fn overview_of(desks: &Desktops) -> DesktopOverview {
    let count = desks.count();
    DesktopOverview {
        active: desks.active(),
        count,
        windows: (0..count)
            .map(|index| desks.manager(index).map_or(0, |manager| manager.len()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::input::ShortcutTable;
    use desilium_config_types::{Action, CompositorSettings};
    use desilium_wm::{Rect, Size};
    use smithay::input::keyboard::ModifiersState;
    use smithay::input::keyboard::xkb::{self, Keysym};

    /// Distinct window ids, minted the way the compositor mints them.
    fn ids(count: usize) -> Vec<WindowId> {
        let mut desks = Desktops::new(Rect::new(0, 0, 800, 600), 1);
        (0..count)
            .map(|_| desks.open_window(Size { w: 10, h: 10 }))
            .collect()
    }

    fn keysym(name: &str) -> Keysym {
        xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS)
    }

    fn mods(alt: bool, ctrl: bool, shift: bool) -> ModifiersState {
        ModifiersState {
            alt,
            ctrl,
            shift,
            ..ModifiersState::default()
        }
    }

    fn lookup(state: &ModifiersState, key: &str) -> Option<Action> {
        ShortcutTable::from_settings(&CompositorSettings::default()).lookup(state, &[keysym(key)])
    }

    #[test]
    fn windows_on_other_desktops_leave_the_view() {
        let ids = ids(4);
        let windows = [(ids[0], 0), (ids[1], 1), (ids[2], 0), (ids[3], 2)];
        assert_eq!(windows_leaving_view(&windows, 0), vec![ids[1], ids[3]]);
        assert_eq!(
            windows_leaving_view(&windows, 2),
            vec![ids[0], ids[1], ids[2]]
        );
    }

    #[test]
    fn nothing_leaves_when_no_window_is_elsewhere() {
        assert!(windows_leaving_view(&[], 0).is_empty());
        let ids = ids(1);
        assert!(windows_leaving_view(&[(ids[0], 1)], 1).is_empty());
    }

    #[test]
    fn neighbours_wrap_in_both_directions() {
        assert_eq!(neighbour_desktop(0, 4, DesktopStep::Next), 1);
        assert_eq!(neighbour_desktop(3, 4, DesktopStep::Next), 0);
        assert_eq!(neighbour_desktop(0, 4, DesktopStep::Prev), 3);
        assert_eq!(neighbour_desktop(2, 4, DesktopStep::Prev), 1);
    }

    #[test]
    fn single_or_empty_desktop_list_stays_put() {
        assert_eq!(neighbour_desktop(0, 1, DesktopStep::Next), 0);
        assert_eq!(neighbour_desktop(0, 1, DesktopStep::Prev), 0);
        assert_eq!(neighbour_desktop(0, 0, DesktopStep::Next), 0);
    }

    #[test]
    fn overview_reports_active_count_and_windows_per_desktop() {
        let area = Rect::new(0, 0, 800, 600);
        let mut desks = Desktops::new(area, 3);
        desks.open_window(Size { w: 10, h: 10 });
        desks.open_window(Size { w: 10, h: 10 });
        desks.switch_to(2);
        desks.open_window(Size { w: 10, h: 10 });
        assert_eq!(
            overview_of(&desks),
            DesktopOverview {
                active: 2,
                count: 3,
                windows: vec![2, 0, 1],
            }
        );
    }

    #[test]
    fn overview_of_single_desktop_is_empty_but_valid() {
        let desks = Desktops::new(Rect::new(0, 0, 800, 600), 1);
        assert_eq!(
            overview_of(&desks),
            DesktopOverview {
                active: 0,
                count: 1,
                windows: vec![0],
            }
        );
    }

    #[test]
    fn default_shortcuts_switch_and_move_between_desktops() {
        let alt = mods(true, false, false);
        let alt_shift = mods(true, false, true);
        let alt_ctrl = mods(true, true, false);
        assert_eq!(lookup(&alt_ctrl, "Right"), Some(Action::SwitchDesktopNext));
        assert_eq!(lookup(&alt_ctrl, "Left"), Some(Action::SwitchDesktopPrev));
        assert_eq!(lookup(&alt, "1"), Some(Action::SwitchToDesktop1));
        assert_eq!(lookup(&alt, "4"), Some(Action::SwitchToDesktop4));
        assert_eq!(lookup(&alt_shift, "1"), Some(Action::MoveToDesktop1));
        assert_eq!(lookup(&alt_shift, "4"), Some(Action::MoveToDesktop4));
    }

    #[test]
    fn plain_arrow_keys_still_snap_not_switch() {
        assert_eq!(
            lookup(&mods(true, false, false), "Right"),
            Some(Action::SnapRight)
        );
        assert_eq!(lookup(&mods(false, true, false), "Right"), None);
    }
}
