//! Root configuration document: the single struct persisted as `config.toml`.

use serde::{Deserialize, Serialize};

use crate::settings::CompositorSettings;

/// Schema version written by this build. Older files are migrated up to it.
pub const CURRENT_VERSION: u32 = 1;
/// Theme used when none is configured.
pub const DEFAULT_THEME: &str = "desilium-dark";
/// Theme id that follows the user's KDE colour scheme instead of a built-in theme.
pub const KDE_THEME: &str = "kde";
/// Wallpaper scaling used when none is configured.
pub const DEFAULT_WALLPAPER_MODE: &str = "fill";
/// Neutral animation speed multiplier.
pub const DEFAULT_ANIMATION_SPEED: f32 = 1.0;
/// Cursor size in logical pixels.
pub const DEFAULT_CURSOR_SIZE: u32 = 24;
/// Default corner radius of windows and large surfaces (the brand value of the theme).
pub const DEFAULT_CORNER_RADIUS: u16 = 12;
/// Default corner radius of buttons and inputs (the brand value of the theme).
pub const DEFAULT_CONTROL_RADIUS: u16 = 8;

/// The complete user configuration, persisted as TOML.
///
/// Sensitive values (passwords, tokens, keys) must never be stored here; they
/// belong in a credential store, because this file is plain text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesiliumConfig {
    /// Schema version of the document.
    pub version: u32,
    pub compositor: CompositorSettings,
    pub appearance: AppearanceSettings,
    pub shell: crate::ShellSettings,
}

impl Default for DesiliumConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            compositor: CompositorSettings::default(),
            appearance: AppearanceSettings::default(),
            shell: crate::ShellSettings::default(),
        }
    }
}

/// Visual settings: theme, accent, wallpaper, animations and cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    /// Name of the theme to load: a built-in id, or [`KDE_THEME`] to follow KDE's colour scheme.
    pub theme: String,
    /// Icon theme for application icons; `None` follows KDE's `[Icons] Theme`, then `hicolor`.
    pub icon_theme: Option<String>,
    /// Hex accent color (`#RRGGBB` or `#RRGGBBAA`) overriding the theme's accent.
    pub accent: Option<String>,
    /// Path to the wallpaper image.
    pub wallpaper: Option<String>,
    /// How the wallpaper is scaled: `fill`, `fit`, `stretch`, `center` or `tile`.
    pub wallpaper_mode: String,
    /// Whether window and desktop animations run.
    pub animations: bool,
    /// Animation speed multiplier; `1.0` is normal speed.
    pub animation_speed: f32,
    /// Replace slides, zooms and tweens with short fades (accessibility).
    pub reduce_motion: bool,
    /// Cursor theme name; `None` uses the system default.
    pub cursor_theme: Option<String>,
    /// Cursor size in logical pixels.
    pub cursor_size: u32,
    /// Corner radius of windows and large surfaces in logical pixels.
    pub corner_radius: u16,
    /// Corner radius of buttons, inputs and list rows in logical pixels.
    pub control_radius: u16,
    /// KDE colour scheme (the name of a `color-schemes/<name>.colors` file) used while `theme` is
    /// [`KDE_THEME`]; `None` follows the scheme named in the user's `kdeglobals`.
    pub kde_scheme: Option<String>,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_owned(),
            icon_theme: None,
            accent: None,
            wallpaper: None,
            wallpaper_mode: DEFAULT_WALLPAPER_MODE.to_owned(),
            animations: true,
            animation_speed: DEFAULT_ANIMATION_SPEED,
            reduce_motion: false,
            cursor_theme: None,
            cursor_size: DEFAULT_CURSOR_SIZE,
            corner_radius: DEFAULT_CORNER_RADIUS,
            control_radius: DEFAULT_CONTROL_RADIUS,
            kde_scheme: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_motion_defaults_to_off_when_absent() {
        let parsed: AppearanceSettings =
            toml::from_str("theme = \"desilium-light\"").expect("parse");
        assert!(!parsed.reduce_motion);
    }

    #[test]
    fn old_files_without_an_icon_theme_follow_kde() {
        let parsed: AppearanceSettings = toml::from_str("theme = \"kde\"").expect("parse");
        assert_eq!(parsed.theme, KDE_THEME);
        assert_eq!(parsed.icon_theme, None);
        let named: AppearanceSettings = toml::from_str("icon_theme = \"Papirus\"").expect("parse");
        assert_eq!(named.icon_theme.as_deref(), Some("Papirus"));
    }

    #[test]
    fn reduce_motion_round_trips() {
        let original = AppearanceSettings {
            reduce_motion: true,
            ..AppearanceSettings::default()
        };
        let text = toml::to_string(&original).expect("serialize");
        let parsed: AppearanceSettings = toml::from_str(&text).expect("parse");
        assert!(parsed.reduce_motion);
    }
}
