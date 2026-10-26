//! The theme the settings window itself wears: the same resolution the compositor applies.

use desilium_config_types::AppearanceSettings;
use desilium_theme::{Color, Mode, Theme};

const LIGHT_THEME_ID: &str = "desilium-light";

/// Light or dark, from the configured theme id (unknown ids fall back to dark like the compositor).
pub fn mode_of(theme_id: &str) -> Mode {
    if theme_id == LIGHT_THEME_ID {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// Built-in theme for the configured id; a valid accent derives the whole palette from it.
pub fn resolve_theme(appearance: &AppearanceSettings) -> Theme {
    let mode = mode_of(&appearance.theme);
    let accent = appearance
        .accent
        .as_deref()
        .and_then(|hex| Color::from_hex(hex).ok());
    match accent {
        Some(color) => Theme::derive_from_accent(color, mode),
        None => Theme::for_mode(mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(theme: &str, accent: Option<&str>) -> AppearanceSettings {
        AppearanceSettings {
            theme: theme.to_owned(),
            accent: accent.map(str::to_owned),
            ..AppearanceSettings::default()
        }
    }

    #[test]
    fn default_is_the_dark_builtin() {
        assert_eq!(resolve_theme(&AppearanceSettings::default()), Theme::dark());
    }

    #[test]
    fn light_id_gives_the_light_theme_and_unknown_ids_dark() {
        assert_eq!(
            resolve_theme(&with("desilium-light", None)).mode,
            Mode::Light
        );
        assert_eq!(resolve_theme(&with("mystery", None)).mode, Mode::Dark);
    }

    #[test]
    fn a_valid_accent_changes_the_palette_and_an_invalid_one_is_ignored() {
        let pink = resolve_theme(&with("desilium-dark", Some("#E8348A")));
        assert_eq!(pink.palette.accent.to_hex().to_ascii_uppercase(), "#E8348A");
        assert_eq!(
            resolve_theme(&with("desilium-dark", Some("nope"))),
            Theme::dark()
        );
    }
}
