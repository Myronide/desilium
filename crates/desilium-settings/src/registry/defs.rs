//! The settings table. Add a setting here and it shows up everywhere.

use desilium_config_types::DesiliumConfig;

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

const DESKTOPS: Stepper = Stepper { min: 1, max: 32 };

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
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
