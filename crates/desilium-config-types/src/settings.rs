use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::shortcut::{Modifiers, Shortcut};

/// Number of virtual desktops when the configuration does not say.
pub const DEFAULT_DESKTOPS: u8 = 4;
/// Smallest and largest accepted desktop counts (mirrors `desilium_wm`).
pub const MIN_DESKTOPS: u8 = 1;
pub const MAX_DESKTOPS: u8 = 32;

/// Keys for desktops 1 to 4: keysym name, action to show it, action to move the window there.
const DESKTOP_KEYS: [(&str, Action, Action); 4] = [
    ("1", Action::SwitchToDesktop1, Action::MoveToDesktop1),
    ("2", Action::SwitchToDesktop2, Action::MoveToDesktop2),
    ("3", Action::SwitchToDesktop3, Action::MoveToDesktop3),
    ("4", Action::SwitchToDesktop4, Action::MoveToDesktop4),
];

/// User-facing compositor settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorSettings {
    pub shortcuts: Vec<Shortcut>,
    /// `None` means auto-detect a terminal emulator at launch time.
    pub terminal: Option<String>,
    /// Number of virtual desktops, `MIN_DESKTOPS..=MAX_DESKTOPS`.
    pub desktops: u8,
}

impl Default for CompositorSettings {
    fn default() -> Self {
        // Alt is the default instead of Logo because the compositor is developed
        // nested inside another desktop, which already claims the Logo key.
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let alt_shift = Modifiers { shift: true, ..alt };
        let alt_ctrl = Modifiers { ctrl: true, ..alt };
        let mut shortcuts = vec![
            Shortcut::new(alt, "q", Action::CloseFocusedWindow),
            Shortcut::new(alt, "Return", Action::LaunchTerminal),
            Shortcut::new(alt_shift, "q", Action::Quit),
            Shortcut::new(alt, "Up", Action::ToggleMaximize),
            Shortcut::new(alt, "f", Action::ToggleFullscreen),
            Shortcut::new(alt, "Down", Action::MinimizeWindow),
            Shortcut::new(alt, "Left", Action::SnapLeft),
            Shortcut::new(alt, "Right", Action::SnapRight),
            Shortcut::new(alt, "Tab", Action::CycleWindowsForward),
            Shortcut::new(alt_shift, "Tab", Action::CycleWindowsBackward),
            Shortcut::new(alt_shift, "m", Action::RestoreMinimized),
            Shortcut::new(alt_ctrl, "Right", Action::SwitchDesktopNext),
            Shortcut::new(alt_ctrl, "Left", Action::SwitchDesktopPrev),
        ];
        shortcuts.extend(DESKTOP_KEYS.iter().flat_map(|(key, switch, moved)| {
            [
                Shortcut::new(alt, key, *switch),
                Shortcut::new(alt_shift, key, *moved),
            ]
        }));
        Self {
            shortcuts,
            terminal: None,
            desktops: DEFAULT_DESKTOPS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_core_shortcuts_first_then_window_actions() {
        let settings = CompositorSettings::default();
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let alt_shift = Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::default()
        };
        let alt_ctrl = Modifiers {
            alt: true,
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(
            settings.shortcuts,
            vec![
                Shortcut::new(alt, "q", Action::CloseFocusedWindow),
                Shortcut::new(alt, "Return", Action::LaunchTerminal),
                Shortcut::new(alt_shift, "q", Action::Quit),
                Shortcut::new(alt, "Up", Action::ToggleMaximize),
                Shortcut::new(alt, "f", Action::ToggleFullscreen),
                Shortcut::new(alt, "Down", Action::MinimizeWindow),
                Shortcut::new(alt, "Left", Action::SnapLeft),
                Shortcut::new(alt, "Right", Action::SnapRight),
                Shortcut::new(alt, "Tab", Action::CycleWindowsForward),
                Shortcut::new(alt_shift, "Tab", Action::CycleWindowsBackward),
                Shortcut::new(alt_shift, "m", Action::RestoreMinimized),
                Shortcut::new(alt_ctrl, "Right", Action::SwitchDesktopNext),
                Shortcut::new(alt_ctrl, "Left", Action::SwitchDesktopPrev),
                Shortcut::new(alt, "1", Action::SwitchToDesktop1),
                Shortcut::new(alt_shift, "1", Action::MoveToDesktop1),
                Shortcut::new(alt, "2", Action::SwitchToDesktop2),
                Shortcut::new(alt_shift, "2", Action::MoveToDesktop2),
                Shortcut::new(alt, "3", Action::SwitchToDesktop3),
                Shortcut::new(alt_shift, "3", Action::MoveToDesktop3),
                Shortcut::new(alt, "4", Action::SwitchToDesktop4),
                Shortcut::new(alt_shift, "4", Action::MoveToDesktop4),
            ]
        );
        assert_eq!(settings.terminal, None);
        assert_eq!(settings.desktops, DEFAULT_DESKTOPS);
    }

    #[test]
    fn desktop_count_defaults_when_absent_and_parses_when_given() {
        let absent: CompositorSettings = toml::from_str("terminal = \"foot\"").expect("parse");
        assert_eq!(absent.desktops, DEFAULT_DESKTOPS);
        let given: CompositorSettings = toml::from_str("desktops = 6").expect("parse");
        assert_eq!(given.desktops, 6);
    }

    #[test]
    fn desktop_actions_use_kebab_case_names() {
        let text = r#"
            [[shortcuts]]
            key = "3"
            action = "move-to-desktop3"
            [shortcuts.modifiers]
            alt = true
            shift = true
        "#;
        let parsed: CompositorSettings = toml::from_str(text).expect("parse");
        assert_eq!(parsed.shortcuts[0].action, Action::MoveToDesktop3);
    }

    #[test]
    fn default_window_action_keys_are_unique() {
        let shortcuts = CompositorSettings::default().shortcuts;
        for (index, shortcut) in shortcuts.iter().enumerate() {
            let duplicate = shortcuts[index + 1..]
                .iter()
                .any(|other| other.modifiers == shortcut.modifiers && other.key == shortcut.key);
            assert!(!duplicate, "{} is bound twice", shortcut.key);
        }
    }

    #[test]
    fn window_actions_use_kebab_case_names() {
        let text = r#"
            [[shortcuts]]
            key = "Tab"
            action = "cycle-windows-backward"
            [shortcuts.modifiers]
            alt = true
        "#;
        let parsed: CompositorSettings = toml::from_str(text).expect("parse");
        assert_eq!(parsed.shortcuts[0].action, Action::CycleWindowsBackward);
    }

    #[test]
    fn default_settings_survive_toml_round_trip() {
        let original = CompositorSettings::default();
        let text = toml::to_string(&original).expect("serialize defaults");
        let parsed: CompositorSettings = toml::from_str(&text).expect("parse defaults");
        assert_eq!(parsed, original);
    }

    #[test]
    fn empty_document_yields_defaults() {
        let parsed: CompositorSettings = toml::from_str("").expect("parse empty");
        assert_eq!(parsed, CompositorSettings::default());
    }

    #[test]
    fn partial_document_keeps_default_shortcuts() {
        let parsed: CompositorSettings = toml::from_str("terminal = \"foot\"").expect("parse");
        assert_eq!(parsed.terminal.as_deref(), Some("foot"));
        assert_eq!(parsed.shortcuts, CompositorSettings::default().shortcuts);
    }

    #[test]
    fn missing_modifier_flags_default_to_false() {
        let text = r#"
            [[shortcuts]]
            key = "t"
            action = "launch-terminal"
            [shortcuts.modifiers]
            ctrl = true
        "#;
        let parsed: CompositorSettings = toml::from_str(text).expect("parse");
        assert_eq!(
            parsed.shortcuts,
            vec![Shortcut::new(
                Modifiers {
                    ctrl: true,
                    ..Modifiers::default()
                },
                "t",
                Action::LaunchTerminal,
            )]
        );
    }

    #[test]
    fn unknown_action_fails_without_panicking() {
        let text = r#"
            [[shortcuts]]
            key = "x"
            action = "explode"
            [shortcuts.modifiers]
        "#;
        assert!(toml::from_str::<CompositorSettings>(text).is_err());
    }
}
