//! Root configuration document: the single struct persisted as `config.toml`.

use serde::{Deserialize, Serialize};

use crate::settings::CompositorSettings;

/// Schema version written by this build. Older files are migrated up to it.
pub const CURRENT_VERSION: u32 = 1;
/// Theme used when none is configured.
pub const DEFAULT_THEME: &str = "desilium-dark";
/// Wallpaper scaling used when none is configured.
pub const DEFAULT_WALLPAPER_MODE: &str = "fill";
/// Neutral animation speed multiplier.
pub const DEFAULT_ANIMATION_SPEED: f32 = 1.0;
/// Cursor size in logical pixels.
pub const DEFAULT_CURSOR_SIZE: u32 = 24;

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
}

impl Default for DesiliumConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            compositor: CompositorSettings::default(),
            appearance: AppearanceSettings::default(),
        }
    }
}

/// Visual settings: theme, accent, wallpaper, animations and cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    /// Name of the theme to load.
    pub theme: String,
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
    /// Cursor theme name; `None` uses the system default.
    pub cursor_theme: Option<String>,
    /// Cursor size in logical pixels.
    pub cursor_size: u32,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_owned(),
            accent: None,
            wallpaper: None,
            wallpaper_mode: DEFAULT_WALLPAPER_MODE.to_owned(),
            animations: true,
            animation_speed: DEFAULT_ANIMATION_SPEED,
            cursor_theme: None,
            cursor_size: DEFAULT_CURSOR_SIZE,
        }
    }
}
