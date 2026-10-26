//! Clamps and repairs configuration values so that every committed config is usable.
//!
//! Every rule has a safe repair, so validation never rejects a config outright.
//! Each repair is reported with a human-readable message for the settings UI.

use std::fmt::Display;

use desilium_config_types::{
    Action, AppearanceSettings, CompositorSettings, DEFAULT_ANIMATION_SPEED, DEFAULT_THEME,
    DEFAULT_WALLPAPER_MODE, DesiliumConfig, MAX_DESKTOPS, MIN_DESKTOPS, Shortcut,
};

const ANIMATION_SPEED_MIN: f32 = 0.25;
const ANIMATION_SPEED_MAX: f32 = 4.0;
const CURSOR_SIZE_MIN: u32 = 16;
const CURSOR_SIZE_MAX: u32 = 128;
const WALLPAPER_MODES: [&str; 5] = ["fill", "fit", "stretch", "center", "tile"];
const HEX_RGB_DIGITS: usize = 6;
const HEX_RGBA_DIGITS: usize = 8;

/// One automatic change made during validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repair {
    /// Dotted path of the affected setting, e.g. `appearance.cursor_size`.
    pub field: String,
    /// Human-readable explanation of what changed.
    pub message: String,
}

/// All repairs made by one validation pass. Empty when the config was already valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub repairs: Vec<Repair>,
}

impl ValidationReport {
    /// True when no value had to be changed.
    pub fn is_clean(&self) -> bool {
        self.repairs.is_empty()
    }

    fn record(&mut self, field: &str, message: String) {
        self.repairs.push(Repair {
            field: field.to_owned(),
            message,
        });
    }
}

/// Repairs `cfg` in place and reports every change.
pub fn validate(cfg: &mut DesiliumConfig) -> ValidationReport {
    let mut report = ValidationReport::default();
    validate_appearance(&mut cfg.appearance, &mut report);
    validate_compositor(&mut cfg.compositor, &mut report);
    validate_shell(&mut cfg.shell, &mut report);
    report
}

fn validate_shell(
    settings: &mut desilium_config_types::ShellSettings,
    report: &mut ValidationReport,
) {
    clamp_field(&mut settings.icon_size, 32, 72, "shell.icon_size", report);
    clamp_field(&mut settings.spacing, 0, 24, "shell.spacing", report);
    clamp_field(&mut settings.margin, 0, 40, "shell.margin", report);
    clamp_field(&mut settings.opacity, 40, 100, "shell.opacity", report);
}

fn validate_appearance(appearance: &mut AppearanceSettings, report: &mut ValidationReport) {
    repair_theme(appearance, report);
    repair_accent(appearance, report);
    clear_if_blank(&mut appearance.wallpaper, "appearance.wallpaper", report);
    repair_wallpaper_mode(appearance, report);
    repair_animation_speed(appearance, report);
    clear_if_blank(
        &mut appearance.cursor_theme,
        "appearance.cursor_theme",
        report,
    );
    clamp_field(
        &mut appearance.cursor_size,
        CURSOR_SIZE_MIN,
        CURSOR_SIZE_MAX,
        "appearance.cursor_size",
        report,
    );
}

fn repair_theme(appearance: &mut AppearanceSettings, report: &mut ValidationReport) {
    if !appearance.theme.trim().is_empty() {
        return;
    }
    appearance.theme = DEFAULT_THEME.to_owned();
    report.record(
        "appearance.theme",
        format!("theme name was empty; using {DEFAULT_THEME}"),
    );
}

fn repair_accent(appearance: &mut AppearanceSettings, report: &mut ValidationReport) {
    let Some(accent) = appearance.accent.as_deref() else {
        return;
    };
    if is_hex_color(accent) {
        return;
    }
    report.record(
        "appearance.accent",
        format!("accent \"{accent}\" is not a hex color; the theme accent is used instead"),
    );
    appearance.accent = None;
}

fn repair_wallpaper_mode(appearance: &mut AppearanceSettings, report: &mut ValidationReport) {
    if WALLPAPER_MODES.contains(&appearance.wallpaper_mode.as_str()) {
        return;
    }
    report.record(
        "appearance.wallpaper_mode",
        format!(
            "unknown wallpaper mode \"{}\"; using {DEFAULT_WALLPAPER_MODE}",
            appearance.wallpaper_mode
        ),
    );
    appearance.wallpaper_mode = DEFAULT_WALLPAPER_MODE.to_owned();
}

fn repair_animation_speed(appearance: &mut AppearanceSettings, report: &mut ValidationReport) {
    if !appearance.animation_speed.is_finite() {
        report.record(
            "appearance.animation_speed",
            format!("animation speed was not a number; using {DEFAULT_ANIMATION_SPEED}"),
        );
        appearance.animation_speed = DEFAULT_ANIMATION_SPEED;
        return;
    }
    clamp_field(
        &mut appearance.animation_speed,
        ANIMATION_SPEED_MIN,
        ANIMATION_SPEED_MAX,
        "appearance.animation_speed",
        report,
    );
}

fn validate_compositor(compositor: &mut CompositorSettings, report: &mut ValidationReport) {
    clear_if_blank(&mut compositor.terminal, "compositor.terminal", report);
    clamp_field(
        &mut compositor.desktops,
        MIN_DESKTOPS,
        MAX_DESKTOPS,
        "compositor.desktops",
        report,
    );
    validate_window_controls(compositor, report);
    repair_shortcuts(&mut compositor.shortcuts, report);
}

fn validate_window_controls(compositor: &mut CompositorSettings, report: &mut ValidationReport) {
    clamp_field(
        &mut compositor.snap_distance,
        4,
        96,
        "compositor.snap_distance",
        report,
    );
    clamp_field(
        &mut compositor.keyboard_repeat_delay,
        100,
        1500,
        "compositor.keyboard_repeat_delay",
        report,
    );
    clamp_field(
        &mut compositor.keyboard_repeat_rate,
        1,
        100,
        "compositor.keyboard_repeat_rate",
        report,
    );
}

fn repair_shortcuts(shortcuts: &mut Vec<Shortcut>, report: &mut ValidationReport) {
    trim_and_drop_keyless(shortcuts, report);
    drop_duplicates(shortcuts, report);
    restore_default_actions(shortcuts, report);
}

/// Trims key names and removes shortcuts without a key.
fn trim_and_drop_keyless(shortcuts: &mut Vec<Shortcut>, report: &mut ValidationReport) {
    let mut kept = Vec::with_capacity(shortcuts.len());
    for mut shortcut in std::mem::take(shortcuts) {
        let trimmed = shortcut.key.trim().to_owned();
        if trimmed.is_empty() {
            report.record(
                "compositor.shortcuts",
                format!(
                    "a shortcut for \"{}\" had no key and was removed",
                    action_label(shortcut.action)
                ),
            );
            continue;
        }
        if trimmed != shortcut.key {
            report.record(
                "compositor.shortcuts",
                format!(
                    "shortcut key \"{}\" was trimmed to \"{trimmed}\"",
                    shortcut.key
                ),
            );
            shortcut.key = trimmed;
        }
        kept.push(shortcut);
    }
    *shortcuts = kept;
}

/// Keeps the first shortcut for each key combination.
fn drop_duplicates(shortcuts: &mut Vec<Shortcut>, report: &mut ValidationReport) {
    let mut kept: Vec<Shortcut> = Vec::with_capacity(shortcuts.len());
    for shortcut in std::mem::take(shortcuts) {
        if kept.iter().any(|other| same_combo(other, &shortcut)) {
            report.record(
                "compositor.shortcuts",
                format!(
                    "shortcut \"{}\" was bound twice; the first binding is kept",
                    shortcut.key
                ),
            );
            continue;
        }
        kept.push(shortcut);
    }
    *shortcuts = kept;
}

/// Re-adds the default shortcut for any core action the user removed.
///
/// A default displaces a user shortcut on the same key combination, so the
/// core actions stay reachable from the keyboard. Window actions are optional
/// and are never restored once the user has removed them.
fn restore_default_actions(shortcuts: &mut Vec<Shortcut>, report: &mut ValidationReport) {
    for default in CompositorSettings::default()
        .shortcuts
        .into_iter()
        .filter(|default| is_core_action(default.action))
    {
        if shortcuts
            .iter()
            .any(|shortcut| shortcut.action == default.action)
        {
            continue;
        }
        let before = shortcuts.len();
        shortcuts.retain(|shortcut| !same_combo(shortcut, &default));
        if shortcuts.len() < before {
            report.record(
                "compositor.shortcuts",
                format!(
                    "shortcut \"{}\" was reassigned to restore \"{}\"",
                    default.key,
                    action_label(default.action)
                ),
            );
        }
        report.record(
            "compositor.shortcuts",
            format!(
                "\"{}\" had no shortcut; restored the default \"{}\"",
                action_label(default.action),
                default.key
            ),
        );
        shortcuts.push(default);
    }
}

fn is_core_action(action: Action) -> bool {
    matches!(
        action,
        Action::CloseFocusedWindow | Action::LaunchTerminal | Action::Quit
    )
}

fn same_combo(first: &Shortcut, second: &Shortcut) -> bool {
    first.modifiers == second.modifiers && first.key == second.key
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::CloseFocusedWindow => "close window",
        Action::LaunchTerminal => "launch terminal",
        Action::Quit => "quit",
        Action::ToggleMaximize => "toggle maximize",
        Action::ToggleFullscreen => "toggle fullscreen",
        Action::MinimizeWindow => "minimize window",
        Action::SnapLeft => "snap left",
        Action::SnapRight => "snap right",
        Action::CycleWindowsForward => "cycle windows forward",
        Action::CycleWindowsBackward => "cycle windows backward",
        Action::RestoreMinimized => "restore minimized window",
        Action::SwitchDesktopNext => "switch to next desktop",
        Action::SwitchDesktopPrev => "switch to previous desktop",
        Action::SwitchToDesktop1 => "switch to desktop 1",
        Action::SwitchToDesktop2 => "switch to desktop 2",
        Action::SwitchToDesktop3 => "switch to desktop 3",
        Action::SwitchToDesktop4 => "switch to desktop 4",
        Action::MoveToDesktop1 => "move window to desktop 1",
        Action::MoveToDesktop2 => "move window to desktop 2",
        Action::MoveToDesktop3 => "move window to desktop 3",
        Action::MoveToDesktop4 => "move window to desktop 4",
        Action::ToggleLauncher => "toggle launcher",
    }
}

fn clear_if_blank(value: &mut Option<String>, field: &str, report: &mut ValidationReport) {
    if !value.as_deref().is_some_and(|text| text.trim().is_empty()) {
        return;
    }
    *value = None;
    report.record(field, format!("{field} was empty and has been cleared"));
}

fn clamp_field<T>(value: &mut T, min: T, max: T, field: &str, report: &mut ValidationReport)
where
    T: PartialOrd + Copy + Display,
{
    if *value >= min && *value <= max {
        return;
    }
    let clamped = if *value < min { min } else { max };
    report.record(
        field,
        format!("{field} {value} is outside {min}..={max}; set to {clamped}"),
    );
    *value = clamped;
}

fn is_hex_color(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), HEX_RGB_DIGITS | HEX_RGBA_DIGITS)
        && digits.chars().all(|digit| digit.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_config_types::{DEFAULT_CURSOR_SIZE, Modifiers};

    fn valid() -> DesiliumConfig {
        DesiliumConfig::default()
    }

    fn fields(report: &ValidationReport) -> Vec<&str> {
        report.repairs.iter().map(|r| r.field.as_str()).collect()
    }

    fn alt_key(key: &str, action: Action) -> Shortcut {
        Shortcut::new(
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            key,
            action,
        )
    }

    #[test]
    fn defaults_are_clean() {
        let mut cfg = valid();
        let report = validate(&mut cfg);
        assert!(report.is_clean(), "{report:?}");
        assert_eq!(cfg, valid());
    }

    #[test]
    fn animation_speed_is_clamped_both_ways() {
        let mut cfg = valid();
        cfg.appearance.animation_speed = 9.0;
        validate(&mut cfg);
        assert_eq!(cfg.appearance.animation_speed, ANIMATION_SPEED_MAX);

        cfg.appearance.animation_speed = 0.01;
        validate(&mut cfg);
        assert_eq!(cfg.appearance.animation_speed, ANIMATION_SPEED_MIN);
    }

    #[test]
    fn non_finite_animation_speed_resets_to_default() {
        let mut cfg = valid();
        cfg.appearance.animation_speed = f32::NAN;
        let report = validate(&mut cfg);
        assert_eq!(cfg.appearance.animation_speed, DEFAULT_ANIMATION_SPEED);
        assert_eq!(fields(&report), vec!["appearance.animation_speed"]);
    }

    #[test]
    fn cursor_size_is_clamped_with_message() {
        let mut cfg = valid();
        cfg.appearance.cursor_size = 500;
        let report = validate(&mut cfg);
        assert_eq!(cfg.appearance.cursor_size, CURSOR_SIZE_MAX);
        assert!(report.repairs[0].message.contains("set to 128"));
    }

    #[test]
    fn cursor_size_boundaries_are_kept() {
        for size in [CURSOR_SIZE_MIN, DEFAULT_CURSOR_SIZE, CURSOR_SIZE_MAX] {
            let mut cfg = valid();
            cfg.appearance.cursor_size = size;
            assert!(validate(&mut cfg).is_clean());
            assert_eq!(cfg.appearance.cursor_size, size);
        }
    }

    #[test]
    fn invalid_accent_is_dropped_and_valid_accent_kept() {
        let mut cfg = valid();
        cfg.appearance.accent = Some("#ff8800".to_owned());
        assert!(validate(&mut cfg).is_clean());

        cfg.appearance.accent = Some("orange".to_owned());
        let report = validate(&mut cfg);
        assert_eq!(cfg.appearance.accent, None);
        assert_eq!(fields(&report), vec!["appearance.accent"]);
    }

    #[test]
    fn hex_colors_need_six_or_eight_digits() {
        assert!(is_hex_color("#abcdef"));
        assert!(is_hex_color("#ABCDEF80"));
        assert!(!is_hex_color("#abcde"));
        assert!(!is_hex_color("abcdef"));
        assert!(!is_hex_color("#abcdeg"));
    }

    #[test]
    fn blank_optional_strings_are_cleared() {
        let mut cfg = valid();
        cfg.appearance.wallpaper = Some("  ".to_owned());
        cfg.appearance.cursor_theme = Some(String::new());
        cfg.compositor.terminal = Some(" ".to_owned());
        let report = validate(&mut cfg);
        assert_eq!(cfg.appearance.wallpaper, None);
        assert_eq!(cfg.appearance.cursor_theme, None);
        assert_eq!(cfg.compositor.terminal, None);
        assert_eq!(report.repairs.len(), 3);
    }

    #[test]
    fn empty_theme_and_unknown_wallpaper_mode_fall_back() {
        let mut cfg = valid();
        cfg.appearance.theme = String::new();
        cfg.appearance.wallpaper_mode = "spiral".to_owned();
        let report = validate(&mut cfg);
        assert_eq!(cfg.appearance.theme, DEFAULT_THEME);
        assert_eq!(cfg.appearance.wallpaper_mode, DEFAULT_WALLPAPER_MODE);
        assert_eq!(report.repairs.len(), 2);
    }

    #[test]
    fn shortcut_keys_are_trimmed_and_keyless_ones_removed() {
        let mut cfg = valid();
        cfg.compositor.shortcuts = vec![
            alt_key(" t ", Action::LaunchTerminal),
            alt_key("  ", Action::CloseFocusedWindow),
        ];
        let report = validate(&mut cfg);
        let keys: Vec<&str> = cfg
            .compositor
            .shortcuts
            .iter()
            .map(|s| s.key.as_str())
            .collect();
        assert_eq!(keys, vec!["t", "q", "q"]);
        assert_eq!(cfg.compositor.shortcuts[0].action, Action::LaunchTerminal);
        assert!(report.repairs.len() >= 3);
    }

    #[test]
    fn duplicate_shortcut_keeps_first() {
        let mut cfg = valid();
        cfg.compositor.shortcuts = vec![
            alt_key("x", Action::LaunchTerminal),
            alt_key("x", Action::Quit),
            alt_key("Return", Action::LaunchTerminal),
            alt_key("q", Action::CloseFocusedWindow),
        ];
        let report = validate(&mut cfg);
        let actions: Vec<Action> = cfg.compositor.shortcuts.iter().map(|s| s.action).collect();
        // Duplicate "x" binding for Quit is dropped; Quit is then restored on alt+shift+q.
        assert_eq!(
            actions,
            vec![
                Action::LaunchTerminal,
                Action::LaunchTerminal,
                Action::CloseFocusedWindow,
                Action::Quit,
            ]
        );
        assert!(
            report
                .repairs
                .iter()
                .any(|r| r.message.contains("bound twice"))
        );
        assert_eq!(
            cfg.compositor
                .shortcuts
                .iter()
                .filter(|s| s.key == "x")
                .count(),
            1
        );
    }

    #[test]
    fn removed_quit_and_terminal_are_restored() {
        let mut cfg = valid();
        cfg.compositor.shortcuts = vec![alt_key("q", Action::CloseFocusedWindow)];
        validate(&mut cfg);
        for action in [
            Action::CloseFocusedWindow,
            Action::LaunchTerminal,
            Action::Quit,
        ] {
            assert!(cfg.compositor.shortcuts.iter().any(|s| s.action == action));
        }
    }

    #[test]
    fn removed_window_actions_are_not_restored() {
        let mut cfg = valid();
        cfg.compositor.shortcuts = vec![alt_key("q", Action::CloseFocusedWindow)];
        validate(&mut cfg);
        assert!(
            !cfg.compositor
                .shortcuts
                .iter()
                .any(|s| s.action == Action::ToggleMaximize || s.action == Action::SnapLeft)
        );
    }

    #[test]
    fn restoring_default_displaces_user_binding_on_same_combo() {
        let mut cfg = valid();
        cfg.compositor.shortcuts = vec![
            alt_key("q", Action::CloseFocusedWindow),
            alt_key("Return", Action::CloseFocusedWindow),
        ];
        let report = validate(&mut cfg);
        let quit = cfg
            .compositor
            .shortcuts
            .iter()
            .find(|s| s.action == Action::Quit);
        assert!(quit.is_some());
        assert!(
            report
                .repairs
                .iter()
                .any(|r| r.message.contains("restored"))
        );
    }

    #[test]
    fn desktop_count_is_clamped_to_supported_range() {
        let mut cfg = valid();
        cfg.compositor.desktops = 0;
        let report = validate(&mut cfg);
        assert_eq!(cfg.compositor.desktops, MIN_DESKTOPS);
        assert_eq!(fields(&report), vec!["compositor.desktops"]);

        cfg.compositor.desktops = 200;
        validate(&mut cfg);
        assert_eq!(cfg.compositor.desktops, MAX_DESKTOPS);
    }

    #[test]
    fn toggle_launcher_is_optional_and_never_restored() {
        let mut cfg = valid();
        cfg.compositor
            .shortcuts
            .retain(|s| s.action != Action::ToggleLauncher);
        let report = validate(&mut cfg);
        assert!(
            !cfg.compositor
                .shortcuts
                .iter()
                .any(|s| s.action == Action::ToggleLauncher)
        );
        assert!(report.is_clean(), "{report:?}");
    }

    #[test]
    fn toggle_launcher_label_is_user_facing() {
        assert_eq!(action_label(Action::ToggleLauncher), "toggle launcher");
    }

    #[test]
    fn default_space_binding_is_clean() {
        let mut cfg = valid();
        assert!(validate(&mut cfg).is_clean());
        assert!(
            cfg.compositor
                .shortcuts
                .iter()
                .any(|s| s.action == Action::ToggleLauncher && s.key == "space")
        );
    }

    #[test]
    fn desktop_count_bounds_are_kept() {
        for count in [MIN_DESKTOPS, MAX_DESKTOPS] {
            let mut cfg = valid();
            cfg.compositor.desktops = count;
            assert!(validate(&mut cfg).is_clean());
        }
    }
}

#[cfg(test)]
mod window_controls_tests {
    use super::*;

    #[test]
    fn window_control_bounds_are_repaired_and_reported() {
        for (distance, delay, rate, expected) in [
            (0, 0, 0, (4, 100, 1)),
            (u16::MAX, u16::MAX, u16::MAX, (96, 1500, 100)),
        ] {
            let mut config = DesiliumConfig::default();
            config.compositor.snap_distance = distance;
            config.compositor.keyboard_repeat_delay = delay;
            config.compositor.keyboard_repeat_rate = rate;
            let report = validate(&mut config);
            let settings = &config.compositor;
            assert_eq!(
                (
                    settings.snap_distance,
                    settings.keyboard_repeat_delay,
                    settings.keyboard_repeat_rate
                ),
                expected
            );
            assert_eq!(report.repairs.len(), 3);
            assert!(validate(&mut config).is_clean());
        }
    }
}
