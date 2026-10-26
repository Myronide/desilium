//! The settings table. Add a setting here and it shows up everywhere.

use desilium_config_types::{
    DesiliumConfig, FocusPolicy, KeyboardLayout, PanelEdge, PanelMode, Visibility,
};

use super::{Choice, Control, Page, SettingDef, Slider, Stepper, value::Value};
use crate::shortcuts;

const THEMES: &[Choice] = &[
    Choice::with_icon("desilium-light", "Light", "sun"),
    Choice::with_icon("desilium-dark", "Dark", "moon"),
];

const FIT_MODES: &[Choice] = &[
    Choice::new("fill", "Fill"),
    Choice::new("fit", "Fit"),
    Choice::new("stretch", "Stretch"),
    Choice::new("center", "Center"),
    Choice::new("tile", "Tile"),
];

const ANIMATION_SPEED: Slider = Slider {
    stops: &[0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0],
    labels: &["Off", "0.25x", "", "", "1x", "", "2x", "", "4x"],
    unit: "x",
};

const CURSOR_SIZE: Slider = Slider {
    stops: &[16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 128.0],
    labels: &["16", "24", "32", "48", "64", "96", "128"],
    unit: "px",
};

const PANEL_MODES: &[Choice] = &[
    Choice::new("dock", "Dock"),
    Choice::new("taskbar", "Taskbar"),
];
const PANEL_EDGES: &[Choice] = &[Choice::new("bottom", "Bottom"), Choice::new("top", "Top")];
const VISIBILITY_MODES: &[Choice] = &[
    Choice::new("always", "Always visible"),
    Choice::new("auto-hide", "Auto-hide"),
    Choice::new("avoid-windows", "Dodge windows"),
];
const KEYBOARD_LAYOUTS: &[Choice] = &[
    Choice::new("de", "Deutsch (DE)"),
    Choice::new("us", "English (US)"),
];
const FOCUS_MODES: &[Choice] = &[
    Choice::new("click", "Click"),
    Choice::new("sloppy", "Pointer"),
];

const REPEAT_DELAY: Slider = Slider {
    stops: &[
        100.0, 200.0, 250.0, 300.0, 400.0, 500.0, 750.0, 1000.0, 1500.0,
    ],
    labels: &[
        "100", "200", "250", "300", "400", "500", "750", "1000", "1500",
    ],
    unit: "ms",
};

const DESKTOPS: Stepper = Stepper { min: 1, max: 32 };

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

fn visibility_name(mode: Visibility) -> &'static str {
    match mode {
        Visibility::Always => "always",
        Visibility::AutoHide => "auto-hide",
        Visibility::AvoidWindows => "avoid-windows",
    }
}

fn parse_visibility(name: &str) -> Option<Visibility> {
    [
        Visibility::Always,
        Visibility::AutoHide,
        Visibility::AvoidWindows,
    ]
    .into_iter()
    .find(|mode| visibility_name(*mode) == name)
}

fn optional_text(value: &Option<String>) -> Value {
    value.as_deref().map_or(Value::Unset, text)
}

fn into_optional(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// Number to `u32`; control validation has already bounded it.
fn to_u32(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) if number.is_finite() => {
            Some(number.round().clamp(0.0, f64::from(u32::MAX)) as u32)
        }
        _ => None,
    }
}

fn get_animation_speed(config: &DesiliumConfig) -> Value {
    let appearance = &config.appearance;
    if !appearance.animations {
        return Value::Number(0.0);
    }
    Value::Number(f64::from(appearance.animation_speed))
}

/// "Off" maps to `animations = false`; every other stop switches animations on at that speed.
fn set_animation_speed(config: &mut DesiliumConfig, value: &Value) {
    let Value::Number(number) = value else {
        return;
    };
    if *number <= 0.0 {
        config.appearance.animations = false;
        return;
    }
    config.appearance.animations = true;
    config.appearance.animation_speed = *number as f32;
}

/// Resetting the shortcuts setting means "back to the built-in list"; any other value is ignored,
/// because individual bindings are edited through the keyboard page, not as a registry value.
fn restore_default_shortcuts(config: &mut DesiliumConfig, value: &Value) {
    let defaults = shortcuts::defaults();
    if *value == Value::Text(shortcuts::fingerprint(&defaults)) {
        config.compositor.shortcuts = defaults;
    }
}

pub(super) static ALL: &[SettingDef] = &[
    SettingDef {
        id: "appearance.theme",
        page: Page::Appearance,
        section: "Style",
        title: "Color scheme",
        description: "Light or dark windows, panels and menus",
        keywords: &["dark mode", "light mode", "theme", "night", "brightness"],
        control: Control::Segmented(THEMES),
        get: |c| text(&c.appearance.theme),
        set: |c, v| {
            if let Value::Text(id) = v {
                c.appearance.theme = id.clone();
            }
        },
    },
    SettingDef {
        id: "appearance.accent",
        page: Page::Appearance,
        section: "Accent color",
        title: "Accent color",
        description: "Highlights, selections and focus rings. Clear it to use the theme's own accent.",
        keywords: &["colour", "highlight", "color", "violet", "palette", "hex"],
        control: Control::Accent,
        get: |c| optional_text(&c.appearance.accent),
        set: |c, v| c.appearance.accent = into_optional(v),
    },
    SettingDef {
        id: "appearance.wallpaper",
        page: Page::Appearance,
        section: "Wallpaper",
        title: "Wallpaper",
        description: "The picture behind your windows",
        keywords: &["background", "desktop picture", "image", "photo"],
        control: Control::Wallpaper,
        get: |c| optional_text(&c.appearance.wallpaper),
        set: |c, v| c.appearance.wallpaper = into_optional(v),
    },
    SettingDef {
        id: "appearance.wallpaper_mode",
        page: Page::Appearance,
        section: "Wallpaper",
        title: "Fit mode",
        description: "How the picture is scaled to the screen",
        keywords: &["scale", "stretch", "tile", "crop", "fill", "center"],
        control: Control::Select(FIT_MODES),
        get: |c| text(&c.appearance.wallpaper_mode),
        set: |c, v| {
            if let Value::Text(mode) = v {
                c.appearance.wallpaper_mode = mode.clone();
            }
        },
    },
    SettingDef {
        id: "appearance.animation_speed",
        page: Page::Appearance,
        section: "Motion",
        title: "Animation speed",
        description: "How fast windows and menus move. Off removes animations.",
        keywords: &[
            "animations",
            "speed",
            "transitions",
            "slow",
            "fast",
            "motion",
        ],
        control: Control::Slider(ANIMATION_SPEED),
        get: get_animation_speed,
        set: set_animation_speed,
    },
    SettingDef {
        id: "appearance.reduce_motion",
        page: Page::Appearance,
        section: "Motion",
        title: "Reduce motion",
        description: "Replace slides and zooms with quick fades",
        keywords: &["accessibility", "vestibular", "fade", "animations"],
        control: Control::Switch,
        get: |c| Value::Bool(c.appearance.reduce_motion),
        set: |c, v| {
            if let Value::Bool(on) = v {
                c.appearance.reduce_motion = *on;
            }
        },
    },
    SettingDef {
        id: "appearance.cursor_theme",
        page: Page::Appearance,
        section: "Pointer",
        title: "Cursor theme",
        description: "The look of the mouse pointer",
        keywords: &["mouse", "pointer", "cursor"],
        control: Control::CursorTheme,
        get: |c| optional_text(&c.appearance.cursor_theme),
        set: |c, v| c.appearance.cursor_theme = into_optional(v),
    },
    SettingDef {
        id: "appearance.cursor_size",
        page: Page::Appearance,
        section: "Pointer",
        title: "Cursor size",
        description: "Larger pointers are easier to find",
        keywords: &["mouse", "pointer", "big", "small", "accessibility"],
        control: Control::Slider(CURSOR_SIZE),
        get: |c| Value::Number(f64::from(c.appearance.cursor_size)),
        set: |c, v| {
            if let Some(size) = to_u32(v) {
                c.appearance.cursor_size = size;
            }
        },
    },
    SettingDef {
        id: "compositor.desktops",
        page: Page::Desktop,
        section: "Virtual desktops",
        title: "Number of desktops",
        description: "Separate workspaces to keep windows apart",
        keywords: &["workspaces", "virtual desktops", "spaces", "count"],
        control: Control::Stepper(DESKTOPS),
        get: |c| Value::Number(f64::from(c.compositor.desktops)),
        set: |c, v| {
            if let Some(count) = to_u32(v) {
                c.compositor.desktops = u8::try_from(count).unwrap_or(u8::MAX);
            }
        },
    },
    SettingDef {
        id: "compositor.terminal",
        page: Page::Desktop,
        section: "Applications",
        title: "Terminal",
        description: "The program that opens with Alt+Enter. Empty detects one automatically.",
        keywords: &["console", "shell", "foot", "kitty", "alacritty", "launch"],
        control: Control::Text {
            placeholder: "Detect automatically",
        },
        get: |c| optional_text(&c.compositor.terminal),
        set: |c, v| c.compositor.terminal = into_optional(v),
    },
    SettingDef {
        id: "compositor.focus",
        page: Page::Desktop,
        section: "Window focus",
        title: "Focus mode",
        description: "Focus windows by clicking or moving the pointer over them",
        keywords: &["floating", "mouse", "sloppy", "activation"],
        control: Control::Segmented(FOCUS_MODES),
        get: |config| {
            text(match config.compositor.focus {
                FocusPolicy::Click => "click",
                FocusPolicy::Sloppy => "sloppy",
            })
        },
        set: |config, value| {
            if let Value::Text(mode) = value {
                config.compositor.focus = match mode.as_str() {
                    "click" => FocusPolicy::Click,
                    "sloppy" => FocusPolicy::Sloppy,
                    _ => return,
                };
            }
        },
    },
    SettingDef {
        id: "compositor.raise_on_focus",
        page: Page::Desktop,
        section: "Window focus",
        title: "Raise on pointer focus",
        description: "Bring a window forward when the pointer gives it focus",
        keywords: &["stacking", "mouse", "floating"],
        control: Control::Switch,
        get: |config| Value::Bool(config.compositor.raise_on_focus),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.compositor.raise_on_focus = *enabled;
            }
        },
    },
    SettingDef {
        id: "compositor.snap_enabled",
        page: Page::Desktop,
        section: "Window snapping",
        title: "Snap at screen edges",
        description: "Drag windows to an edge or corner to arrange them; keyboard shortcuts stay available",
        keywords: &["floating", "snap", "halves", "quarters"],
        control: Control::Switch,
        get: |config| Value::Bool(config.compositor.snap_enabled),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.compositor.snap_enabled = *enabled;
            }
        },
    },
    SettingDef {
        id: "compositor.snap_distance",
        page: Page::Desktop,
        section: "Window snapping",
        title: "Edge snap distance (px)",
        description: "Pointer distance from an edge that activates snapping; corner zones keep their own size",
        keywords: &["threshold", "snap", "sensitivity"],
        control: Control::Stepper(Stepper { min: 4, max: 96 }),
        get: |config| Value::Number(f64::from(config.compositor.snap_distance)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.compositor.snap_distance = number;
            }
        },
    },
    SettingDef {
        id: "compositor.keyboard_repeat_delay",
        page: Page::Keyboard,
        section: "Key repeat",
        title: "Repeat delay",
        description: "Time before a held key starts repeating",
        keywords: &["keyboard", "input", "milliseconds", "typing"],
        control: Control::Slider(REPEAT_DELAY),
        get: |config| Value::Number(f64::from(config.compositor.keyboard_repeat_delay)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.compositor.keyboard_repeat_delay = number;
            }
        },
    },
    SettingDef {
        id: "compositor.keyboard_repeat_rate",
        page: Page::Keyboard,
        section: "Key repeat",
        title: "Repeat rate (Hz)",
        description: "Characters per second while a key is held",
        keywords: &["keyboard", "input", "speed", "typing"],
        control: Control::Stepper(Stepper { min: 1, max: 100 }),
        get: |config| Value::Number(f64::from(config.compositor.keyboard_repeat_rate)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.compositor.keyboard_repeat_rate = number;
            }
        },
    },
    SettingDef {
        id: "shell.restore_session",
        page: Page::Desktop,
        section: "Session",
        title: "Restore windows at startup",
        description: "Reopen the applications you had open, on their workspaces and at their old positions. Program contents such as browser tabs are restored by the programs themselves",
        keywords: &["resume", "reopen", "login", "startup", "windows", "macos"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.restore_session),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.restore_session = *enabled;
            }
        },
    },
    SettingDef {
        id: "compositor.keyboard_layout",
        page: Page::Keyboard,
        section: "Layout",
        title: "Keyboard layout",
        description: "Key mapping for all applications; also switchable from the top bar",
        keywords: &["language", "german", "english", "qwertz", "qwerty", "input", "de", "us"],
        control: Control::Segmented(KEYBOARD_LAYOUTS),
        get: |config| text(config.compositor.keyboard_layout.xkb_name()),
        set: |config, value| {
            if let Value::Text(name) = value
                && let Some(layout) = KeyboardLayout::ALL
                    .into_iter()
                    .find(|layout| layout.xkb_name() == name)
            {
                config.compositor.keyboard_layout = layout;
            }
        },
    },
    SettingDef {
        id: "compositor.shortcuts",
        page: Page::Keyboard,
        section: "Shortcuts",
        title: "Keyboard shortcuts",
        description: "Every shortcut the desktop reacts to",
        keywords: &[
            "keys",
            "hotkeys",
            "bindings",
            "alt",
            "tab",
            "close window",
            "snap",
        ],
        control: Control::Shortcuts,
        get: |c| Value::Text(shortcuts::fingerprint(&c.compositor.shortcuts)),
        set: restore_default_shortcuts,
    },
    SettingDef {
        id: "shell.panel_enabled",
        page: Page::Panel,
        section: "Visibility",
        title: "Show dock / taskbar",
        description: "Turn the dock or taskbar off completely",
        keywords: &["dock", "taskbar", "panel", "disable", "hide"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.panel_enabled),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.panel_enabled = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.panel_visibility",
        page: Page::Panel,
        section: "Visibility",
        title: "Dock / taskbar behavior",
        description: "Always visible, hidden until the pointer touches the edge, or hidden while a window overlaps it",
        keywords: &["dock", "taskbar", "panel", "autohide", "hide", "dodge", "intellihide"],
        control: Control::Segmented(VISIBILITY_MODES),
        get: |config| text(visibility_name(config.shell.panel_visibility)),
        set: |config, value| {
            if let Value::Text(name) = value
                && let Some(mode) = parse_visibility(name)
            {
                config.shell.panel_visibility = mode;
            }
        },
    },
    SettingDef {
        id: "shell.bar_enabled",
        page: Page::Panel,
        section: "Visibility",
        title: "Show top bar",
        description: "Turn the top bar off completely",
        keywords: &["top bar", "panel", "disable", "hide"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.bar_enabled),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.bar_enabled = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.bar_visibility",
        page: Page::Panel,
        section: "Visibility",
        title: "Top bar behavior",
        description: "Always visible, hidden until the pointer touches the edge, or hidden while a window overlaps it",
        keywords: &["top bar", "panel", "autohide", "hide", "dodge", "intellihide"],
        control: Control::Segmented(VISIBILITY_MODES),
        get: |config| text(visibility_name(config.shell.bar_visibility)),
        set: |config, value| {
            if let Value::Text(name) = value
                && let Some(mode) = parse_visibility(name)
            {
                config.shell.bar_visibility = mode;
            }
        },
    },
    SettingDef {
        id: "shell.mode",
        page: Page::Panel,
        section: "Layout",
        title: "Panel mode",
        description: "Floating application dock or full-width taskbar with application labels",
        keywords: &["dock", "taskbar", "panel"],
        control: Control::Segmented(PANEL_MODES),
        get: |config| {
            text(match config.shell.mode {
                PanelMode::Dock => "dock",
                PanelMode::Taskbar => "taskbar",
            })
        },
        set: |config, value| {
            if let Value::Text(text) = value {
                config.shell.mode = match text.as_str() {
                    "dock" => PanelMode::Dock,
                    "taskbar" => PanelMode::Taskbar,
                    _ => return,
                };
            }
        },
    },
    SettingDef {
        id: "shell.edge",
        page: Page::Panel,
        section: "Layout",
        title: "Position",
        description: "Attach the dock or taskbar to the top or bottom edge",
        keywords: &["dock", "taskbar", "panel"],
        control: Control::Segmented(PANEL_EDGES),
        get: |config| {
            text(match config.shell.edge {
                PanelEdge::Bottom => "bottom",
                PanelEdge::Top => "top",
            })
        },
        set: |config, value| {
            if let Value::Text(text) = value {
                config.shell.edge = match text.as_str() {
                    "bottom" => PanelEdge::Bottom,
                    "top" => PanelEdge::Top,
                    _ => return,
                };
            }
        },
    },
    SettingDef {
        id: "shell.icon_size",
        page: Page::Panel,
        section: "Appearance",
        title: "Icon size (px)",
        description: "Application icon size and panel height",
        keywords: &["dock", "taskbar", "size", "appearance"],
        control: Control::Stepper(Stepper { min: 32, max: 72 }),
        get: |config| Value::Number(f64::from(config.shell.icon_size)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.shell.icon_size = number;
            }
        },
    },
    SettingDef {
        id: "shell.spacing",
        page: Page::Panel,
        section: "Appearance",
        title: "Item spacing (px)",
        description: "Space between application buttons",
        keywords: &["dock", "taskbar", "size", "appearance"],
        control: Control::Stepper(Stepper { min: 0, max: 24 }),
        get: |config| Value::Number(f64::from(config.shell.spacing)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.shell.spacing = number;
            }
        },
    },
    SettingDef {
        id: "shell.margin",
        page: Page::Panel,
        section: "Appearance",
        title: "Edge margin (px)",
        description: "Distance between the panel and the screen edge",
        keywords: &["dock", "taskbar", "size", "appearance"],
        control: Control::Stepper(Stepper { min: 0, max: 40 }),
        get: |config| Value::Number(f64::from(config.shell.margin)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.shell.margin = number;
            }
        },
    },
    SettingDef {
        id: "shell.opacity",
        page: Page::Panel,
        section: "Appearance",
        title: "Panel opacity (%)",
        description: "Opacity of the panel background",
        keywords: &["dock", "taskbar", "size", "appearance"],
        control: Control::Stepper(Stepper { min: 40, max: 100 }),
        get: |config| Value::Number(f64::from(config.shell.opacity)),
        set: |config, value| {
            if let Some(number) = to_u32(value).and_then(|number| u16::try_from(number).ok()) {
                config.shell.opacity = number;
            }
        },
    },
    SettingDef {
        id: "shell.show_labels",
        page: Page::Panel,
        section: "Content",
        title: "Taskbar labels",
        description: "Show application names beside taskbar icons",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_labels),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_labels = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.show_running",
        page: Page::Panel,
        section: "Content",
        title: "Show running applications",
        description: "Include unpinned applications while they are running",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_running),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_running = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.show_indicators",
        page: Page::Panel,
        section: "Content",
        title: "Running indicators",
        description: "Show dots for open and focused windows",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_indicators),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_indicators = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.show_workspaces",
        page: Page::Panel,
        section: "Content",
        title: "Workspace switcher",
        description: "Show workspace buttons in the top panel",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_workspaces),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_workspaces = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.show_clock",
        page: Page::Panel,
        section: "Content",
        title: "Show clock",
        description: "Show the current time in panels",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_clock),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_clock = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.clock_24h",
        page: Page::Panel,
        section: "Content",
        title: "24-hour clock",
        description: "Use 24-hour time instead of AM/PM",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.clock_24h),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.clock_24h = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.show_battery",
        page: Page::Panel,
        section: "Content",
        title: "Battery status",
        description: "Show the battery indicator when a battery is present",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.show_battery),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.show_battery = *enabled;
            }
        },
    },
    SettingDef {
        id: "shell.quick_settings",
        page: Page::Panel,
        section: "Content",
        title: "Quick settings",
        description: "Open system controls from the panel status cluster",
        keywords: &["dock", "taskbar", "panel", "customize"],
        control: Control::Switch,
        get: |config| Value::Bool(config.shell.quick_settings),
        set: |config, value| {
            if let Value::Bool(enabled) = value {
                config.shell.quick_settings = *enabled;
            }
        },
    },
    SettingDef {
        id: "about.system",
        page: Page::About,
        section: "Desilium",
        title: "About Desilium",
        description: "Version, configuration location and credits",
        keywords: &[
            "version",
            "info",
            "license",
            "build",
            "config file",
            "location",
        ],
        control: Control::About,
        get: |_| Value::Unset,
        set: |_, _| {},
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(change: impl FnOnce(&mut DesiliumConfig)) -> DesiliumConfig {
        let mut config = DesiliumConfig::default();
        change(&mut config);
        config
    }

    #[test]
    fn animation_speed_off_disables_animations_and_keeps_the_speed() {
        let mut config = config_with(|c| c.appearance.animation_speed = 2.0);
        set_animation_speed(&mut config, &Value::Number(0.0));
        assert!(!config.appearance.animations);
        assert_eq!(config.appearance.animation_speed, 2.0);
        assert_eq!(get_animation_speed(&config), Value::Number(0.0));
    }

    #[test]
    fn choosing_a_speed_turns_animations_back_on() {
        let mut config = config_with(|c| c.appearance.animations = false);
        set_animation_speed(&mut config, &Value::Number(1.5));
        assert!(config.appearance.animations);
        assert_eq!(get_animation_speed(&config), Value::Number(1.5));
    }

    #[test]
    fn defaults_come_from_the_config_defaults() {
        let by_id = |id: &str| super::super::find(id).expect(id);
        assert_eq!(
            by_id("appearance.theme").default_value(),
            text("desilium-dark")
        );
        assert_eq!(by_id("appearance.accent").default_value(), Value::Unset);
        assert_eq!(
            by_id("appearance.animation_speed").default_value(),
            Value::Number(1.0)
        );
        assert_eq!(
            by_id("compositor.desktops").default_value(),
            Value::Number(4.0)
        );
    }

    #[test]
    fn slider_stops_are_within_what_config_validation_accepts() {
        assert!(
            ANIMATION_SPEED
                .stops
                .iter()
                .all(|s| *s == 0.0 || (0.25..=4.0).contains(s))
        );
        assert!(CURSOR_SIZE.stops.iter().all(|s| (16.0..=128.0).contains(s)));
        assert_eq!(ANIMATION_SPEED.stops.len(), ANIMATION_SPEED.labels.len());
        assert_eq!(CURSOR_SIZE.stops.len(), CURSOR_SIZE.labels.len());
    }

    #[test]
    fn writing_a_bad_type_leaves_the_config_alone() {
        let mut config = DesiliumConfig::default();
        let def = super::super::find("appearance.theme").expect("theme");
        (def.set)(&mut config, &Value::Bool(true));
        assert_eq!(config, DesiliumConfig::default());
    }
}
