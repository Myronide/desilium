use serde::{Deserialize, Serialize};

/// A compositor operation that a shortcut can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    CloseFocusedWindow,
    LaunchTerminal,
    Quit,
    ToggleMaximize,
    ToggleFullscreen,
    MinimizeWindow,
    SnapLeft,
    SnapRight,
    /// Focuses the next visible window in stacking order, wrapping around.
    CycleWindowsForward,
    /// Focuses the previous visible window in stacking order, wrapping around.
    CycleWindowsBackward,
    /// Restores the most recently minimized window.
    RestoreMinimized,
    /// Shows the next virtual desktop, wrapping around.
    SwitchDesktopNext,
    /// Shows the previous virtual desktop, wrapping around.
    SwitchDesktopPrev,
    /// Shows virtual desktop 1 (the first one).
    SwitchToDesktop1,
    SwitchToDesktop2,
    SwitchToDesktop3,
    SwitchToDesktop4,
    /// Moves the focused window to virtual desktop 1 and leaves the current desktop in view.
    MoveToDesktop1,
    MoveToDesktop2,
    MoveToDesktop3,
    MoveToDesktop4,
}
