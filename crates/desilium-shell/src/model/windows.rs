//! The compositor's window list as the shell sees it, plus a most-recently-used order.

/// Compositor window id (`desilium_toplevel_handle_v1.window_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u32);

/// Everything the shell knows about one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    /// Stable id.
    pub id: WindowId,
    /// Window title, possibly empty.
    pub title: String,
    /// Wayland app id, possibly empty.
    pub app_id: String,
    /// Icon name the compositor reports for the window (KWin's themed icon), possibly empty.
    pub icon: String,
    /// Has keyboard focus.
    pub activated: bool,
    /// Hidden to the dock.
    pub minimized: bool,
    /// Zero-based workspace (the first one when the window is on all of them).
    pub workspace: u32,
    /// Shown on every workspace.
    pub on_all_workspaces: bool,
    /// Global logical rectangle as `(x, y, width, height)`.
    pub rect: (i32, i32, i32, i32),
}

impl WindowInfo {
    /// A window with no state flags and no title, for tests and as the protocol default.
    pub fn new(id: WindowId) -> Self {
        Self {
            id,
            title: String::new(),
            app_id: String::new(),
            icon: String::new(),
            activated: false,
            minimized: false,
            workspace: 0,
            on_all_workspaces: false,
            rect: (0, 0, 0, 0),
        }
    }

    /// Whether the window is visible on `workspace`.
    pub fn on_workspace(&self, workspace: u32) -> bool {
        self.on_all_workspaces || self.workspace == workspace
    }
}

/// Workspace count and the active one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workspaces {
    /// Number of workspaces, at least 1.
    pub count: u32,
    /// Zero-based active index, below `count`.
    pub active: u32,
}

impl Default for Workspaces {
    fn default() -> Self {
        Self {
            count: 1,
            active: 0,
        }
    }
}

/// The window list in creation order plus the recency of activation.
#[derive(Debug, Clone, Default)]
pub struct WindowModel {
    windows: Vec<WindowInfo>,
    /// Most recently activated first; contains only live windows.
    recent: Vec<WindowId>,
}

impl WindowModel {
    /// Replaces the list (in creation order) and updates the recency order.
    pub fn replace(&mut self, windows: Vec<WindowInfo>) {
        self.windows = windows;
        let live = |id: &WindowId| self.windows.iter().any(|w| w.id == *id);
        self.recent.retain(live);
        let active = self.windows.iter().find(|w| w.activated).map(|w| w.id);
        if let Some(id) = active {
            self.recent.retain(|recent| *recent != id);
            self.recent.insert(0, id);
        }
        for window in &self.windows {
            if !self.recent.contains(&window.id) {
                self.recent.push(window.id);
            }
        }
    }

    /// All windows in creation order.
    pub fn windows(&self) -> &[WindowInfo] {
        &self.windows
    }

    /// The window with `id`.
    pub fn get(&self, id: WindowId) -> Option<&WindowInfo> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// The focused window, if any.
    pub fn active(&self) -> Option<&WindowInfo> {
        self.windows.iter().find(|w| w.activated)
    }

    /// The focused window if it is visible on `workspace`. KWin can keep reporting the window of
    /// the workspace just left as active for a moment; the bar must not show it then.
    pub fn active_on(&self, workspace: u32) -> Option<&WindowInfo> {
        self.active()
            .filter(|window| window.on_workspace(workspace))
    }

    /// Position of `id` in the recency order (0 = most recent).
    pub fn recency(&self, id: WindowId) -> Option<usize> {
        self.recent.iter().position(|recent| *recent == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u32, activated: bool) -> WindowInfo {
        WindowInfo {
            activated,
            ..WindowInfo::new(WindowId(id))
        }
    }

    #[test]
    fn activation_moves_a_window_to_the_front() {
        let mut model = WindowModel::default();
        model.replace(vec![window(1, false), window(2, true)]);
        assert_eq!(model.recency(WindowId(2)), Some(0));
        model.replace(vec![window(1, true), window(2, false)]);
        assert_eq!(model.recency(WindowId(1)), Some(0));
        assert_eq!(model.recency(WindowId(2)), Some(1));
    }

    #[test]
    fn closed_windows_leave_the_recency_list() {
        let mut model = WindowModel::default();
        model.replace(vec![window(1, true), window(2, false)]);
        model.replace(vec![window(2, true)]);
        assert_eq!(model.recency(WindowId(1)), None);
        assert_eq!(model.recency(WindowId(2)), Some(0));
        assert_eq!(model.active().map(|w| w.id), Some(WindowId(2)));
    }

    #[test]
    fn the_bar_shows_only_a_focused_window_of_the_current_workspace() {
        let mut elsewhere = window(1, true);
        elsewhere.workspace = 1;
        let mut everywhere = window(2, true);
        everywhere.workspace = 1;
        everywhere.on_all_workspaces = true;
        let mut model = WindowModel::default();
        model.replace(vec![elsewhere]);
        assert!(model.active_on(0).is_none());
        assert_eq!(model.active_on(1).map(|w| w.id), Some(WindowId(1)));
        model.replace(vec![everywhere]);
        assert_eq!(model.active_on(0).map(|w| w.id), Some(WindowId(2)));
    }

    #[test]
    fn unfocused_new_windows_rank_after_known_ones() {
        let mut model = WindowModel::default();
        model.replace(vec![window(1, true)]);
        model.replace(vec![window(1, true), window(2, false)]);
        assert_eq!(model.recency(WindowId(2)), Some(1));
    }

    #[test]
    fn no_focus_keeps_the_previous_order() {
        let mut model = WindowModel::default();
        model.replace(vec![window(1, false), window(2, true)]);
        model.replace(vec![window(1, false), window(2, false)]);
        assert_eq!(model.recency(WindowId(2)), Some(0));
        assert!(model.active().is_none());
    }
}
