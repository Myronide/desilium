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

/// How a window receives keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    #[default]
    Click,
    Sloppy,
}

/// Keyboard layouts offered in the settings and the top bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardLayout {
    #[default]
    De,
    Us,
}

impl KeyboardLayout {
    /// Every layout, in the order the top bar cycles through them.
    pub const ALL: [Self; 2] = [Self::De, Self::Us];

    /// XKB layout name.
    pub fn xkb_name(self) -> &'static str {
        match self {
            Self::De => "de",
            Self::Us => "us",
        }
    }

    /// Short label for the top bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::De => "DE",
            Self::Us => "US",
        }
    }

    /// The layout after this one, wrapping around.
    pub fn next(self) -> Self {
        match self {
            Self::De => Self::Us,
            Self::Us => Self::De,
        }
    }
}

/// User-facing compositor settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompositorSettings {
    pub shortcuts: Vec<Shortcut>,
    /// `None` means auto-detect a terminal emulator at launch time.
    pub terminal: Option<String>,
    /// Number of virtual desktops, `MIN_DESKTOPS..=MAX_DESKTOPS`.
    pub desktops: u8,
    pub focus: FocusPolicy,
    /// Bring a window forward when it receives pointer focus.
    pub raise_on_focus: bool,
    /// Drag-to-edge snapping; keyboard snapping remains available.
    pub snap_enabled: bool,
    /// Distance from a work area edge in logical pixels.
    pub snap_distance: u16,
    pub keyboard_repeat_delay: u16,
    pub keyboard_repeat_rate: u16,
    pub keyboard_layout: KeyboardLayout,
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
        shortcuts.push(Shortcut::new(alt, "space", Action::ToggleLauncher));
        Self {
            shortcuts,
            terminal: Some("kitty".to_owned()),
            desktops: DEFAULT_DESKTOPS,
            focus: FocusPolicy::Click,
            raise_on_focus: true,
            snap_enabled: true,
            snap_distance: 8,
            keyboard_repeat_delay: 200,
            keyboard_repeat_rate: 25,
            keyboard_layout: KeyboardLayout::De,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_defaults_to_german_and_cycles() {
        assert_eq!(
            CompositorSettings::default().keyboard_layout,
            KeyboardLayout::De
        );
        assert_eq!(KeyboardLayout::De.next(), KeyboardLayout::Us);
        assert_eq!(KeyboardLayout::Us.next(), KeyboardLayout::De);
        assert_eq!(KeyboardLayout::Us.xkb_name(), "us");
    }

    #[test]
    fn old_files_without_a_layout_get_german() {
        let parsed: CompositorSettings = toml::from_str("desktops = 2").expect("parse");
        assert_eq!(parsed.keyboard_layout, KeyboardLayout::De);
    }

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
                Shortcut::new(alt, "space", Action::ToggleLauncher),
            ]
        );
        assert_eq!(settings.terminal.as_deref(), Some("kitty"));
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
    fn toggle_launcher_uses_kebab_case_and_defaults_to_alt_space() {
        let text = r#"
            [[shortcuts]]
            key = "space"
            action = "toggle-launcher"
            [shortcuts.modifiers]
            alt = true
        "#;
        let parsed: CompositorSettings = toml::from_str(text).expect("parse");
        assert_eq!(parsed.shortcuts[0].action, Action::ToggleLauncher);
        let default_binding = CompositorSettings::default()
            .shortcuts
            .into_iter()
            .find(|shortcut| shortcut.action == Action::ToggleLauncher)
            .expect("toggle launcher has a default");
        assert_eq!(default_binding.key, "space");
        assert!(default_binding.modifiers.alt);
        assert!(!default_binding.modifiers.shift && !default_binding.modifiers.ctrl);
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
    #[test]
    fn old_documents_keep_existing_input_and_focus_defaults() {
        let settings: CompositorSettings = toml::from_str("desktops = 6").expect("old document");
        assert_eq!(settings.focus, FocusPolicy::Click);
        assert!(settings.raise_on_focus && settings.snap_enabled);
        assert_eq!(settings.snap_distance, 8);
        assert_eq!(settings.keyboard_repeat_delay, 200);
        assert_eq!(settings.keyboard_repeat_rate, 25);
    }

    #[test]
    fn window_controls_round_trip_and_unknown_focus_is_rejected() {
        let settings = CompositorSettings {
            focus: FocusPolicy::Sloppy,
            raise_on_focus: false,
            snap_enabled: false,
            snap_distance: 24,
            keyboard_repeat_delay: 300,
            keyboard_repeat_rate: 30,
            ..CompositorSettings::default()
        };
        let raw = toml::to_string(&settings).expect("serialize");
        assert_eq!(
            toml::from_str::<CompositorSettings>(&raw).expect("parse"),
            settings
        );
        assert!(toml::from_str::<CompositorSettings>("focus = 'unknown'").is_err());
    }
}
