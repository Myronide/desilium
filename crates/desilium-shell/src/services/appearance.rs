//! Theme selection from the user's configuration, mirroring what the compositor does.

use desilium_config_types::{AppearanceSettings, DEFAULT_THEME, KDE_THEME};
use desilium_theme::{BUILTIN_THEMES, Color, Mode, Theme, kde::KdeThemeError};

const LIGHT_THEME_ID: &str = "desilium-light";

/// Builds the theme for `settings`; `mode_override` (`--theme`) replaces the theme id only.
///
/// Never fails: unknown ids and invalid accents fall back and are logged.
pub fn resolve_theme(settings: &AppearanceSettings, mode_override: Option<Mode>) -> Theme {
    let mut theme = resolve_base(settings, mode_override);
    theme.window.corner_radius = f32::from(settings.corner_radius);
    theme.window.control_radius = f32::from(settings.control_radius);
    theme
}

fn resolve_base(settings: &AppearanceSettings, mode_override: Option<Mode>) -> Theme {
    if mode_override.is_none() && settings.theme == KDE_THEME {
        // The accent comes from the KDE scheme as well.
        return kde_or_dark(desilium_theme::kde::load_theme_for(
            settings.kde_scheme.as_deref(),
        ));
    }
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

/// Icon theme used when neither the environment, the configuration nor KDE name one: KDE's own
/// default. Where it is not installed the lookup simply continues in `hicolor`.
const DEFAULT_ICON_THEME: &str = "breeze";
/// Environment variable that overrides every other icon theme choice.
const ICON_THEME_VAR: &str = "DESILIUM_ICON_THEME";

/// The icon theme to resolve application icons in: `DESILIUM_ICON_THEME`, the configured theme,
/// KDE's `[Icons] Theme`, then `breeze`. The resolver follows the theme's `Inherits` and always
/// ends in `hicolor` and the pixmaps directory, where most applications ship their icon.
pub fn icon_theme(settings: &AppearanceSettings) -> String {
    pick_icon_theme(
        std::env::var(ICON_THEME_VAR).ok().as_deref(),
        settings.icon_theme.as_deref(),
        desilium_theme::kde::icon_theme().as_deref(),
    )
}

fn pick_icon_theme(env: Option<&str>, configured: Option<&str>, kde: Option<&str>) -> String {
    [env, configured, kde]
        .into_iter()
        .flatten()
        .find(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_ICON_THEME)
        .to_owned()
}

/// The KDE theme, or the dark built-in (and a log line) when KDE has no usable scheme.
fn kde_or_dark(loaded: Result<Theme, KdeThemeError>) -> Theme {
    loaded.unwrap_or_else(|error| {
        tracing::warn!(%error, "the KDE colour scheme is unusable, using the dark theme");
        Theme::dark()
    })
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
    fn icon_theme_order_is_environment_configuration_kde_then_breeze() {
        assert_eq!(
            pick_icon_theme(Some("env"), Some("cfg"), Some("kde")),
            "env"
        );
        assert_eq!(pick_icon_theme(None, Some("cfg"), Some("kde")), "cfg");
        assert_eq!(
            pick_icon_theme(None, None, Some("breeze-dark")),
            "breeze-dark"
        );
        assert_eq!(pick_icon_theme(None, None, None), "breeze");
        assert_eq!(
            pick_icon_theme(Some(" "), Some(""), None),
            "breeze",
            "blank names are skipped"
        );
    }

    #[test]
    fn a_missing_kde_scheme_falls_back_to_dark_and_a_found_one_is_used() {
        assert_eq!(kde_or_dark(Err(KdeThemeError::NoScheme)), Theme::dark());
        let light = Theme::light();
        assert_eq!(kde_or_dark(Ok(light.clone())), light);
    }

    #[test]
    fn the_theme_override_beats_the_kde_theme() {
        let forced = resolve_theme(&settings(KDE_THEME, None), Some(Mode::Light));
        assert_eq!(forced.mode, Mode::Light);
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
