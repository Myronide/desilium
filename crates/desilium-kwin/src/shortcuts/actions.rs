//! Which component handles which [`Action`]: KWin's own actions or Desilium's.

use desilium_config_types::Action;

/// KGlobalAccel component that owns Desilium's own actions.
pub const COMPONENT: &str = "desilium";
/// KGlobalAccel component of KWin.
pub const KWIN_COMPONENT: &str = "kwin";

/// Starts the configured terminal; handled by the bridge.
pub const LAUNCH_TERMINAL: &str = "launch-terminal";
/// Ends the session; handled by the bridge.
pub const QUIT: &str = "quit";
/// Shows or hides the launcher; handled by the shell.
pub const TOGGLE_LAUNCHER: &str = "toggle-launcher";
/// Restores the most recently minimized window; handled by the shell.
pub const RESTORE_MINIMIZED: &str = "restore-minimized";

/// Where the shortcut for an action is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// An action KWin already implements, identified by its unique name.
    Kwin(&'static str),
    /// An action of the `desilium` component, identified by its unique name.
    Desilium(&'static str),
}

impl Target {
    /// Component and action names as KGlobalAccel knows them.
    pub fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Kwin(action) => (KWIN_COMPONENT, action),
            Self::Desilium(action) => (COMPONENT, action),
        }
    }

    /// Text shown in KDE's shortcut dialogs.
    pub fn friendly_name(self) -> &'static str {
        match self {
            Self::Kwin(action) => action,
            Self::Desilium(LAUNCH_TERMINAL) => "Launch terminal",
            Self::Desilium(QUIT) => "End the session",
            Self::Desilium(TOGGLE_LAUNCHER) => "Show or hide the launcher",
            Self::Desilium(RESTORE_MINIMIZED) => "Restore the last minimized window",
            Self::Desilium(other) => other,
        }
    }
}

/// The target for `action`. Unique names are KWin 6's.
pub fn target(action: Action) -> Target {
    match action {
        Action::CloseFocusedWindow => Target::Kwin("Window Close"),
        Action::ToggleMaximize => Target::Kwin("Window Maximize"),
        Action::ToggleFullscreen => Target::Kwin("Window Fullscreen"),
        Action::MinimizeWindow => Target::Kwin("Window Minimize"),
        Action::SnapLeft => Target::Kwin("Window Quick Tile Left"),
        Action::SnapRight => Target::Kwin("Window Quick Tile Right"),
        Action::CycleWindowsForward => Target::Kwin("Walk Through Windows"),
        Action::CycleWindowsBackward => Target::Kwin("Walk Through Windows (Reverse)"),
        Action::SwitchDesktopNext => Target::Kwin("Switch to Next Desktop"),
        Action::SwitchDesktopPrev => Target::Kwin("Switch to Previous Desktop"),
        Action::SwitchToDesktop1 => Target::Kwin("Switch to Desktop 1"),
        Action::SwitchToDesktop2 => Target::Kwin("Switch to Desktop 2"),
        Action::SwitchToDesktop3 => Target::Kwin("Switch to Desktop 3"),
        Action::SwitchToDesktop4 => Target::Kwin("Switch to Desktop 4"),
        Action::MoveToDesktop1 => Target::Kwin("Window to Desktop 1"),
        Action::MoveToDesktop2 => Target::Kwin("Window to Desktop 2"),
        Action::MoveToDesktop3 => Target::Kwin("Window to Desktop 3"),
        Action::MoveToDesktop4 => Target::Kwin("Window to Desktop 4"),
        Action::LaunchTerminal => Target::Desilium(LAUNCH_TERMINAL),
        Action::Quit => Target::Desilium(QUIT),
        Action::ToggleLauncher => Target::Desilium(TOGGLE_LAUNCHER),
        Action::RestoreMinimized => Target::Desilium(RESTORE_MINIMIZED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kwin_actions_live_in_the_kwin_component() {
        assert_eq!(
            target(Action::CloseFocusedWindow).names(),
            ("kwin", "Window Close")
        );
    }

    #[test]
    fn desilium_actions_use_the_shared_constants() {
        assert_eq!(
            target(Action::ToggleLauncher).names(),
            ("desilium", TOGGLE_LAUNCHER)
        );
        assert_eq!(target(Action::Quit).names(), ("desilium", QUIT));
        assert_eq!(
            target(Action::LaunchTerminal).names(),
            ("desilium", LAUNCH_TERMINAL)
        );
        assert_eq!(
            target(Action::RestoreMinimized).names(),
            ("desilium", RESTORE_MINIMIZED)
        );
    }

    #[test]
    fn desktop_actions_are_numbered_like_kwin_numbers_them() {
        assert_eq!(
            target(Action::SwitchToDesktop3),
            Target::Kwin("Switch to Desktop 3")
        );
        assert_eq!(
            target(Action::MoveToDesktop4),
            Target::Kwin("Window to Desktop 4")
        );
    }

    #[test]
    fn every_desilium_action_has_a_readable_name() {
        for name in [LAUNCH_TERMINAL, QUIT, TOGGLE_LAUNCHER, RESTORE_MINIMIZED] {
            assert_ne!(Target::Desilium(name).friendly_name(), name);
        }
    }
}
