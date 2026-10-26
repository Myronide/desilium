//! What the shell may do to windows, built on the compositor's own window actions.

use crate::state::DesiliumState;
use desilium_wm::{WindowId, WindowState};

impl DesiliumState {
    /// Switches to the window's desktop, restores it if needed, raises and focuses it.
    pub(super) fn shell_activate_window(&mut self, id: WindowId) {
        let Some(desktop) = self.wm.desktop_of(id) else {
            return;
        };
        self.switch_desktop(desktop);
        if self.wm.window_state(id) == Some(WindowState::Minimized) {
            self.shell_set_minimized(id, false);
            return;
        }
        if !self.wm.focus(id) {
            return;
        }
        self.sync_space();
        self.apply_focus();
    }

    /// Minimizes or restores a window of the desktop in view; other desktops are left alone.
    pub(super) fn shell_set_minimized(&mut self, id: WindowId, minimized: bool) {
        if minimized {
            self.minimize_window(id);
            return;
        }
        if self.wm.window_state(id) != Some(WindowState::Minimized) {
            return;
        }
        if self.wm.restore(id) {
            self.after_window_change(id);
        }
    }

    /// Maximizes or unmaximizes a window of the desktop in view.
    pub(super) fn shell_set_maximized(&mut self, id: WindowId, maximized: bool) {
        let state = self.wm.window_state(id);
        let Some(state) = state.filter(|state| *state != WindowState::Minimized) else {
            return;
        };
        if (state == WindowState::Maximized) == maximized {
            return;
        }
        self.toggle_maximize_window(id);
    }

    /// Moves a window to another desktop; out-of-range indices are ignored.
    pub(super) fn shell_move_to_workspace(&mut self, id: WindowId, index: usize) {
        self.move_window_to_desktop(id, index);
    }
}
