//! Window and desktop actions triggered by shortcuts and client requests.

use super::desktops::DesktopStep;
use crate::state::DesiliumState;
use desilium_config_types::Action;
use desilium_wm::{Placement, SnapZone, WindowId, WindowManager, WindowState};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
use smithay::wayland::shell::xdg::{ToplevelStateSet, ToplevelSurface};

/// What a client asks of its own toplevel through xdg-shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRequest {
    Maximize,
    Unmaximize,
    Fullscreen,
    Unfullscreen,
    Minimize,
}

impl DesiliumState {
    /// Handles every action that is not part of the basic set (close, terminal, quit).
    pub fn run_window_action(&mut self, action: Action) {
        match action {
            // Handled by `run_action` before this is reached.
            Action::CloseFocusedWindow | Action::LaunchTerminal | Action::Quit => {}
            Action::ToggleMaximize => self.with_focused(Self::toggle_maximize_window),
            Action::ToggleFullscreen => self.with_focused(Self::toggle_fullscreen_window),
            Action::MinimizeWindow => self.with_focused(Self::minimize_window),
            Action::SnapLeft => {
                self.with_focused(|state, id| state.snap_window_to(id, SnapZone::Left));
            }
            Action::SnapRight => {
                self.with_focused(|state, id| state.snap_window_to(id, SnapZone::Right));
            }
            Action::CycleWindowsForward => self.cycle_focus(CycleDirection::Forward),
            Action::CycleWindowsBackward => self.cycle_focus(CycleDirection::Backward),
            Action::RestoreMinimized => self.restore_most_recent_minimized(),
            Action::SwitchDesktopNext => self.step_desktop(DesktopStep::Next),
            Action::SwitchDesktopPrev => self.step_desktop(DesktopStep::Prev),
            Action::SwitchToDesktop1 => self.switch_desktop(0),
            Action::SwitchToDesktop2 => self.switch_desktop(1),
            Action::SwitchToDesktop3 => self.switch_desktop(2),
            Action::SwitchToDesktop4 => self.switch_desktop(3),
            Action::MoveToDesktop1 => self.move_focused_window_to_desktop(0),
            Action::MoveToDesktop2 => self.move_focused_window_to_desktop(1),
            Action::MoveToDesktop3 => self.move_focused_window_to_desktop(2),
            Action::MoveToDesktop4 => self.move_focused_window_to_desktop(3),
        }
    }

    /// Asks the window to close (the client decides whether to). Title-bar and keyboard closes
    /// fade the last frame out when animations are on; the destroy path never comes through here.
    pub fn close_window(&self, id: WindowId) {
        self.request_close_animated(id);
    }

    /// Chrome button / double-click on the title bar.
    pub fn toggle_maximize_window(&mut self, id: WindowId) {
        if self.change_window(id, |wm| wm.toggle_maximize(id)) {
            self.after_window_change(id);
        }
    }

    /// Chrome button.
    ///
    /// The window is raised before it is hidden: the window manager has no minimize
    /// timestamp, and raising keeps "most recently minimized" at the top of the stack.
    pub fn minimize_window(&mut self, id: WindowId) {
        if self.wm.window_state(id) == Some(WindowState::Minimized) {
            return;
        }
        if !self.change_window(id, |wm| wm.restore(id) && wm.minimize(id)) {
            return;
        }
        self.after_window_change(id);
    }

    /// Applies a client's maximize/fullscreen/minimize request (the handlers in xdg_shell.rs call this).
    pub fn handle_window_request(&mut self, surface: &ToplevelSurface, request: WindowRequest) {
        let Some(id) = self.window_id_for_surface(surface.wl_surface()) else {
            return;
        };
        match request {
            WindowRequest::Maximize => self.set_maximized_window(id, true),
            WindowRequest::Unmaximize => self.set_maximized_window(id, false),
            WindowRequest::Fullscreen => self.set_fullscreen_window(id, true),
            WindowRequest::Unfullscreen => self.set_fullscreen_window(id, false),
            WindowRequest::Minimize => self.minimize_window(id),
        }
    }

    fn with_focused(&mut self, act: fn(&mut Self, WindowId)) {
        if let Some(id) = self.wm.focused() {
            act(self, id);
        }
    }

    fn set_maximized_window(&mut self, id: WindowId, on: bool) {
        let is_maximized = self.wm.window_state(id) == Some(WindowState::Maximized);
        if is_maximized == on {
            return;
        }
        self.toggle_maximize_window(id);
    }

    fn toggle_fullscreen_window(&mut self, id: WindowId) {
        let on = self.wm.window_state(id) != Some(WindowState::Fullscreen);
        self.set_fullscreen_window(id, on);
    }

    fn set_fullscreen_window(&mut self, id: WindowId, on: bool) {
        let is_fullscreen = self.wm.window_state(id) == Some(WindowState::Fullscreen);
        if is_fullscreen == on || !self.change_window(id, |wm| wm.set_fullscreen(id, on)) {
            return;
        }
        self.after_window_change(id);
    }

    /// Snaps `id` into `zone` through the same path as the keyboard snap actions.
    pub(crate) fn snap_window_to(&mut self, id: WindowId, zone: SnapZone) {
        if self.change_window(id, |wm| wm.snap_window(id, zone)) {
            self.after_window_change(id);
        }
    }

    /// Applies a window-manager change to the desktop that holds `id`, without switching to it.
    /// False when no desktop holds the window or the change was refused.
    fn change_window(
        &mut self,
        id: WindowId,
        change: impl FnOnce(&mut WindowManager) -> bool,
    ) -> bool {
        self.wm.with_window_manager(id, change).unwrap_or(false)
    }

    fn restore_most_recent_minimized(&mut self) {
        let Some(id) = self.wm.minimized_windows().last().copied() else {
            return;
        };
        if self.wm.restore(id) {
            self.after_window_change(id);
        }
    }

    /// Focuses and raises the next visible window; minimized windows are not part of the cycle.
    fn cycle_focus(&mut self, direction: CycleDirection) {
        let placements = self.wm.placements();
        let Some(next) = next_window_in_cycle(&placements, self.wm.focused(), direction) else {
            return;
        };
        if !self.wm.focus(next) {
            return;
        }
        self.sync_space();
        self.apply_focus();
    }

    /// Runs after any window-manager state change: client configure, scene and focus updates.
    pub(crate) fn after_window_change(&mut self, id: WindowId) {
        self.configure_window(id);
        self.unmap_minimized();
        self.sync_space();
        self.apply_focus();
    }

    /// Sends the window's current size and maximized/fullscreen state to its client.
    pub(crate) fn configure_window(&self, id: WindowId) {
        let (Some(rect), Some(state), Some(window)) = (
            self.wm.rect(id),
            self.wm.window_state(id),
            self.windows.get(&id),
        ) else {
            return;
        };
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let client = self.client_size_for_frame(id, rect.size);
        toplevel.with_pending_state(|pending| {
            pending.size = Some((client.w, client.h).into());
            set_flag(
                &mut pending.states,
                State::Maximized,
                state == WindowState::Maximized,
            );
            set_flag(
                &mut pending.states,
                State::Fullscreen,
                state == WindowState::Fullscreen,
            );
        });
        toplevel.send_pending_configure();
    }

    /// Minimized windows must leave the render space so they neither draw nor receive pointer input.
    fn unmap_minimized(&mut self) {
        for id in self.wm.minimized_windows() {
            if let Some(window) = self.windows.get(&id) {
                self.space.unmap_elem(window);
            }
        }
    }
}

/// Which way the window cycle moves through the stacking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleDirection {
    /// Towards the top of the stack.
    Forward,
    /// Towards the bottom of the stack.
    Backward,
}

/// The window to focus next, given placements listed bottom to top.
///
/// Minimized windows are skipped and the cycle wraps around. With no focused
/// visible window, forward starts at the bottom and backward at the top.
/// Returns `None` when no window is visible.
pub fn next_window_in_cycle(
    placements: &[Placement],
    focused: Option<WindowId>,
    direction: CycleDirection,
) -> Option<WindowId> {
    let visible: Vec<WindowId> = placements
        .iter()
        .filter(|placement| placement.state != WindowState::Minimized)
        .map(|placement| placement.id)
        .collect();
    let len = visible.len();
    if len == 0 {
        return None;
    }
    let current = focused.and_then(|id| visible.iter().position(|&other| other == id));
    let Some(current) = current else {
        return match direction {
            CycleDirection::Forward => visible.first().copied(),
            CycleDirection::Backward => visible.last().copied(),
        };
    };
    let index = match direction {
        CycleDirection::Forward => current.saturating_add(1) % len,
        CycleDirection::Backward => current.saturating_add(len).saturating_sub(1) % len,
    };
    visible.get(index).copied()
}

fn set_flag(states: &mut ToplevelStateSet, state: State, on: bool) {
    if on {
        states.set(state);
    } else {
        states.unset(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_wm::{Placement, Rect, WindowManager};

    fn ids(count: usize) -> Vec<WindowId> {
        let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
        (0..count)
            .map(|_| wm.open_window(desilium_wm::Size { w: 10, h: 10 }))
            .collect()
    }

    fn placement(id: WindowId, state: WindowState) -> Placement {
        Placement {
            id,
            rect: Rect::new(0, 0, 10, 10),
            state,
        }
    }

    fn normal(ids: &[WindowId]) -> Vec<Placement> {
        ids.iter()
            .map(|&id| placement(id, WindowState::Normal))
            .collect()
    }

    #[test]
    fn empty_stack_has_no_next_window() {
        let next = next_window_in_cycle(&[], None, CycleDirection::Forward);
        assert_eq!(next, None);
    }

    #[test]
    fn single_window_cycles_to_itself() {
        let ids = ids(1);
        let next = next_window_in_cycle(&normal(&ids), Some(ids[0]), CycleDirection::Forward);
        assert_eq!(next, Some(ids[0]));
        let prev = next_window_in_cycle(&normal(&ids), Some(ids[0]), CycleDirection::Backward);
        assert_eq!(prev, Some(ids[0]));
    }

    #[test]
    fn forward_moves_up_and_wraps_to_bottom() {
        let ids = ids(3);
        let stack = normal(&ids);
        let next = next_window_in_cycle(&stack, Some(ids[0]), CycleDirection::Forward);
        assert_eq!(next, Some(ids[1]));
        let wrapped = next_window_in_cycle(&stack, Some(ids[2]), CycleDirection::Forward);
        assert_eq!(wrapped, Some(ids[0]));
    }

    #[test]
    fn backward_moves_down_and_wraps_to_top() {
        let ids = ids(3);
        let stack = normal(&ids);
        let prev = next_window_in_cycle(&stack, Some(ids[2]), CycleDirection::Backward);
        assert_eq!(prev, Some(ids[1]));
        let wrapped = next_window_in_cycle(&stack, Some(ids[0]), CycleDirection::Backward);
        assert_eq!(wrapped, Some(ids[2]));
    }

    #[test]
    fn minimized_windows_are_skipped() {
        let ids = ids(3);
        let stack = vec![
            placement(ids[0], WindowState::Normal),
            placement(ids[1], WindowState::Minimized),
            placement(ids[2], WindowState::Normal),
        ];
        let next = next_window_in_cycle(&stack, Some(ids[0]), CycleDirection::Forward);
        assert_eq!(next, Some(ids[2]));
        let prev = next_window_in_cycle(&stack, Some(ids[2]), CycleDirection::Backward);
        assert_eq!(prev, Some(ids[0]));
    }

    #[test]
    fn only_minimized_windows_yield_nothing() {
        let ids = ids(2);
        let stack = vec![
            placement(ids[0], WindowState::Minimized),
            placement(ids[1], WindowState::Minimized),
        ];
        assert_eq!(
            next_window_in_cycle(&stack, None, CycleDirection::Forward),
            None
        );
    }

    #[test]
    fn no_focus_starts_at_the_ends() {
        let ids = ids(3);
        let stack = normal(&ids);
        assert_eq!(
            next_window_in_cycle(&stack, None, CycleDirection::Forward),
            Some(ids[0])
        );
        assert_eq!(
            next_window_in_cycle(&stack, None, CycleDirection::Backward),
            Some(ids[2])
        );
    }

    #[test]
    fn focus_on_minimized_window_falls_back_to_the_ends() {
        let ids = ids(3);
        let stack = vec![
            placement(ids[0], WindowState::Normal),
            placement(ids[1], WindowState::Minimized),
            placement(ids[2], WindowState::Normal),
        ];
        assert_eq!(
            next_window_in_cycle(&stack, Some(ids[1]), CycleDirection::Forward),
            Some(ids[0])
        );
    }
}
