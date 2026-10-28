//! Pure resolution of [`AppearanceSettings`] into the theme the KDE colour scheme is made from.
//!
//! No I/O happens here, so every fallback can be unit-tested directly.

use desilium_config_types::{AppearanceSettings, DEFAULT_THEME};
use desilium_theme::{BUILTIN_THEMES, Color, Mode, Theme};

const DARK_THEME_ID: &str = "desilium-dark";
const LIGHT_THEME_ID: &str = "desilium-light";

/// The appearance in effect plus the theme built from it.
#[derive(Debug, Clone)]
pub struct ResolvedAppearance {
    /// Settings as in effect: an unknown theme id or invalid accent is replaced.
    pub appearance: AppearanceSettings,
    /// Validated theme ready for the renderer.
    pub theme: Theme,
    /// One note per fallback or repair; empty when everything was used as configured.
    pub warnings: Vec<String>,
}

/// Resolves `config` into the effective appearance and theme.
///
/// `theme_override` (from `--theme dark|light`) replaces the theme id only; the
/// accent and every other setting still come from `config`.
pub fn resolve_appearance(
    config: &AppearanceSettings,
    theme_override: Option<Mode>,
) -> ResolvedAppearance {
    let mut warnings = Vec::new();
    let theme_id = select_theme_id(&config.theme, theme_override, &mut warnings);
    let built = builtin_theme(theme_id, &mut warnings);
    let (accent, mut theme) = apply_accent(config.accent.as_deref(), built, &mut warnings);
    warnings.extend(theme.validate().iter().map(ToString::to_string));
    let mut appearance = config.clone();
    appearance.theme = theme_id.to_owned();
    appearance.accent = accent;
    ResolvedAppearance {
        appearance,
        theme,
        warnings,
    }
}

fn select_theme_id(
    configured: &str,
    theme_override: Option<Mode>,
    warnings: &mut Vec<String>,
) -> &'static str {
    if let Some(mode) = theme_override {
        return builtin_id_for(mode);
    }
    if let Some(builtin) = BUILTIN_THEMES
        .iter()
        .find(|builtin| builtin.id == configured)
    {
        return builtin.id;
    }

    warnings.push(format!(
        "unknown theme \"{configured}\"; using {DEFAULT_THEME}"
    ));
    DEFAULT_THEME
}

/// The id of the built-in theme for `mode`.
fn builtin_id_for(mode: Mode) -> &'static str {
    match mode {
        Mode::Dark => DARK_THEME_ID,
        Mode::Light => LIGHT_THEME_ID,
    }
}

/// Parses the shipped TOML of a built-in theme. Falls back to the matching Rust built-in.
fn builtin_theme(id: &str, warnings: &mut Vec<String>) -> Theme {
    let mode = mode_of(id);
    let source = BUILTIN_THEMES.iter().find(|builtin| builtin.id == id);
    let Some(builtin) = source else {
        warnings.push(format!("built-in theme {id} is missing"));
        return Theme::for_mode(mode);
    };
    match Theme::from_toml_str(builtin.toml) {
        Ok(theme) => theme,
        Err(error) => {
            warnings.push(format!("built-in theme {id} is unusable: {error}"));
            Theme::for_mode(mode)
        }
    }
}

fn mode_of(id: &str) -> Mode {
    if id == LIGHT_THEME_ID {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// Derives the theme from a valid accent; an invalid accent is dropped with a warning.
fn apply_accent(
    accent: Option<&str>,
    theme: Theme,
    warnings: &mut Vec<String>,
) -> (Option<String>, Theme) {
    let Some(text) = accent else {
        return (None, theme);
    };
    match Color::from_hex(text) {
        Ok(color) => (
            Some(text.to_owned()),
            Theme::derive_from_accent(color, theme.mode),
        ),
        Err(error) => {
            warnings.push(format!("accent \"{text}\" ignored: {error}"));
            (None, theme)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appearance(theme: &str, accent: Option<&str>) -> AppearanceSettings {
        AppearanceSettings {
            theme: theme.to_owned(),
            accent: accent.map(str::to_owned),
            ..AppearanceSettings::default()
        }
    }

    #[test]
    fn default_appearance_resolves_to_the_dark_builtin_without_warnings() {
        let resolved = resolve_appearance(&AppearanceSettings::default(), None);
        assert_eq!(resolved.appearance.theme, "desilium-dark");
        assert_eq!(resolved.theme.mode, Mode::Dark);
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn light_builtin_is_selected_by_id() {
        let resolved = resolve_appearance(&appearance("desilium-light", None), None);
        assert_eq!(resolved.theme.mode, Mode::Light);
        assert_eq!(resolved.appearance.theme, "desilium-light");
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn unknown_theme_id_falls_back_to_dark_with_a_warning() {
        let resolved = resolve_appearance(&appearance("neon-pink", None), None);
        assert_eq!(resolved.theme.mode, Mode::Dark);
        assert_eq!(resolved.appearance.theme, "desilium-dark");
        assert!(
            resolved
                .warnings
                .iter()
                .any(|w| w.contains("unknown theme \"neon-pink\""))
        );
    }

    #[test]
    fn theme_override_wins_over_the_configured_id() {
        let resolved = resolve_appearance(&appearance("desilium-dark", None), Some(Mode::Light));
        assert_eq!(resolved.theme.mode, Mode::Light);
        assert_eq!(resolved.appearance.theme, "desilium-light");
    }

    #[test]
    fn valid_accent_derives_the_palette_and_is_kept() {
        let resolved = resolve_appearance(&appearance("desilium-dark", Some("#E94E77")), None);
        let accent = Color::from_hex("#E94E77").expect("valid hex");
        let expected = Theme::derive_from_accent(accent, Mode::Dark);
        assert_eq!(resolved.theme.palette, expected.palette);
        assert_eq!(resolved.appearance.accent.as_deref(), Some("#E94E77"));
        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
    }

    #[test]
    fn accent_follows_the_mode_of_the_selected_theme() {
        let resolved = resolve_appearance(&appearance("desilium-light", Some("#E94E77")), None);
        assert_eq!(resolved.theme.mode, Mode::Light);
    }

    #[test]
    fn eight_digit_accent_is_accepted() {
        let resolved = resolve_appearance(&appearance("desilium-dark", Some("#7B61FFCC")), None);
        assert_eq!(resolved.appearance.accent.as_deref(), Some("#7B61FFCC"));
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn invalid_accent_is_ignored_and_reported() {
        let builtin = resolve_appearance(&appearance("desilium-dark", None), None);
        for text in ["red", "7B61FF", "#12345", "#GGGGGG"] {
            let resolved = resolve_appearance(&appearance("desilium-dark", Some(text)), None);
            assert_eq!(resolved.appearance.accent, None, "accent {text}");
            assert_eq!(resolved.theme, builtin.theme, "accent {text}");
            assert_eq!(resolved.warnings.len(), 1, "accent {text}");
        }
    }

    #[test]
    fn other_appearance_settings_pass_through_unchanged() {
        let config = AppearanceSettings {
            animations: false,
            animation_speed: 2.0,
            cursor_theme: Some("Adwaita".to_owned()),
            cursor_size: 32,
            wallpaper: Some("/pics/a.png".to_owned()),
            wallpaper_mode: "fit".to_owned(),
            ..AppearanceSettings::default()
        };
        let resolved = resolve_appearance(&config, None);
        assert!(!resolved.appearance.animations);
        assert_eq!(resolved.appearance.animation_speed, 2.0);
        assert_eq!(resolved.appearance.cursor_theme.as_deref(), Some("Adwaita"));
        assert_eq!(resolved.appearance.cursor_size, 32);
        assert_eq!(
            resolved.appearance.wallpaper.as_deref(),
            Some("/pics/a.png")
        );
        assert_eq!(resolved.appearance.wallpaper_mode, "fit");
    }

    #[test]
    fn every_builtin_theme_is_valid_as_shipped() {
        for builtin in BUILTIN_THEMES {
            let resolved = resolve_appearance(&appearance(builtin.id, None), None);
            assert!(
                resolved.warnings.is_empty(),
                "{}: {:?}",
                builtin.id,
                resolved.warnings
            );
        }
    }
}
