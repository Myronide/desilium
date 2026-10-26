//! Theme selection from the user's configuration, mirroring what the compositor does.

use desilium_config_types::{AppearanceSettings, DEFAULT_THEME};
use desilium_theme::{BUILTIN_THEMES, Color, Mode, Theme};

const LIGHT_THEME_ID: &str = "desilium-light";

/// Builds the theme for `settings`; `mode_override` (`--theme`) replaces the theme id only.
///
/// Never fails: unknown ids and invalid accents fall back and are logged.
pub fn resolve_theme(settings: &AppearanceSettings, mode_override: Option<Mode>) -> Theme {
    let id = match mode_override {
        Some(Mode::Light) => LIGHT_THEME_ID,
        Some(Mode::Dark) => DEFAULT_THEME,
        None => settings.theme.as_str(),
    };
    let builtin = BUILTIN_THEMES.iter().find(|builtin| builtin.id == id);
    let theme = match builtin {
        Some(builtin) => Theme::from_toml_str(builtin.toml).unwrap_or_else(|error| {
            tracing::warn!(%error, id, "built-in theme unusable, using the Rust default");
            Theme::for_mode(mode_of(id))
        }),
        None => {
            tracing::warn!(id, "unknown theme, using the default");
            Theme::dark()
        }
    };
    apply_accent(settings.accent.as_deref(), theme)
}

fn mode_of(id: &str) -> Mode {
    if id == LIGHT_THEME_ID {
        Mode::Light
    } else {
        Mode::Dark
    }
}

fn apply_accent(accent: Option<&str>, theme: Theme) -> Theme {
    let Some(text) = accent else {
        return theme;
    };
    match Color::from_hex(text) {
        Ok(color) => Theme::derive_from_accent(color, theme.mode),
        Err(error) => {
            tracing::warn!(%error, accent = text, "invalid accent ignored");
            theme
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(theme: &str, accent: Option<&str>) -> AppearanceSettings {
        AppearanceSettings {
            theme: theme.to_owned(),
            accent: accent.map(str::to_owned),
            ..AppearanceSettings::default()
        }
    }

    #[test]
    fn default_is_dark_and_light_is_selectable() {
        assert_eq!(
            resolve_theme(&AppearanceSettings::default(), None).mode,
            Mode::Dark
        );
        assert_eq!(
            resolve_theme(&settings("desilium-light", None), None).mode,
            Mode::Light
        );
    }

    #[test]
    fn override_wins() {
        let light = resolve_theme(&settings("desilium-dark", None), Some(Mode::Light));
        assert_eq!(light.mode, Mode::Light);
        let dark = resolve_theme(&settings("desilium-light", None), Some(Mode::Dark));
        assert_eq!(dark.mode, Mode::Dark);
    }

    #[test]
    fn accent_changes_the_palette_and_garbage_does_not() {
        let plain = resolve_theme(&settings("desilium-dark", None), None);
        let pink = resolve_theme(&settings("desilium-dark", Some("#E94E77")), None);
        assert_ne!(plain.palette, pink.palette);
        let broken = resolve_theme(&settings("desilium-dark", Some("pink")), None);
        assert_eq!(broken, plain);
    }

    #[test]
    fn unknown_theme_falls_back_to_dark() {
        assert_eq!(
            resolve_theme(&settings("nope", None), None).mode,
            Mode::Dark
        );
    }
}
