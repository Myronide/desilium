//! The complete [`Theme`] and its (de)serialisation.

use serde::{Deserialize, Serialize};

use crate::{
    Background, Color, Effects, Mode, Motion, Palette, Shadow, Spacing, ThemeIssue, Typography,
    Window,
};

/// Failure to load or store a theme document.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    /// The TOML text is malformed or a value has the wrong type.
    #[error("invalid theme document: {0}")]
    Parse(#[from] toml::de::Error),
    /// The theme could not be written as TOML.
    #[error("cannot serialise theme: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Every visual token of the desktop. Missing sections fall back to dark defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default = "Theme::seed")]
pub struct Theme {
    /// Display name shown in the settings GUI.
    pub name: String,
    /// Colour scheme.
    pub mode: Mode,
    /// Semantic colours.
    pub palette: Palette,
    /// Window decorations.
    pub window: Window,
    /// Elevation shadows.
    pub shadow: Shadow,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Fonts and type scale.
    pub typography: Typography,
    /// Durations and easing.
    pub motion: Motion,
    /// Default wallpaper.
    pub background: Background,
    /// Blur and translucency.
    pub effects: Effects,
}

impl Theme {
    /// The default Desilium dark theme.
    pub fn dark() -> Theme {
        Theme::assemble("Desilium Dark", Mode::Dark, Palette::dark())
    }

    /// The Desilium light theme.
    pub fn light() -> Theme {
        Theme::assemble("Desilium Light", Mode::Light, Palette::light())
    }

    /// Built-in theme for `mode`.
    pub fn for_mode(mode: Mode) -> Theme {
        match mode {
            Mode::Dark => Theme::dark(),
            Mode::Light => Theme::light(),
        }
    }

    /// Generates a harmonious, AA-accessible theme from any accent colour.
    pub fn derive_from_accent(accent: Color, mode: Mode) -> Theme {
        let palette = Palette::derive(accent, mode);
        let name = format!("Desilium {mode:?} {}", palette.accent);
        Theme::assemble(&name, mode, palette)
    }

    fn assemble(name: &str, mode: Mode, palette: Palette) -> Theme {
        Theme::build(name, mode, palette).normalized()
    }

    /// Serde seed for missing keys. It must not call `normalized`, which deserialises.
    fn seed() -> Theme {
        Theme::build("Desilium Dark", Mode::Dark, Palette::dark())
    }

    fn build(name: &str, mode: Mode, palette: Palette) -> Theme {
        Theme {
            name: name.to_owned(),
            mode,
            window: Window::for_palette(&palette, mode),
            shadow: Shadow::for_mode(mode),
            spacing: Spacing::default(),
            typography: Typography::default(),
            motion: Motion::default(),
            background: Background::for_accent(palette.accent, mode),
            effects: Effects::for_mode(mode),
            palette,
        }
    }

    /// Snaps all colours to 8-bit so a saved theme reloads identically.
    fn normalized(self) -> Theme {
        let snapped = toml::Value::try_from(&self)
            .ok()
            .and_then(|value| value.try_into().ok());
        snapped.unwrap_or(self)
    }

    /// Parses a (possibly partial) TOML document.
    ///
    /// Missing keys come from the built-in theme matching the document's
    /// `mode`, so `mode = "light"` alone yields a complete light theme.
    pub fn from_toml_str(text: &str) -> Result<Theme, ThemeError> {
        let overlay: toml::Table = text.parse()?;
        let mode = match overlay.get("mode").and_then(toml::Value::as_str) {
            Some("light") => Mode::Light,
            _ => Mode::Dark,
        };
        let mut merged = toml::Table::try_from(Theme::for_mode(mode))?;
        merge_tables(&mut merged, overlay);
        Ok(toml::Value::Table(merged).try_into()?)
    }

    /// Like [`Theme::from_toml_str`], then repairs the result and reports changes.
    pub fn from_toml_str_validated(text: &str) -> Result<(Theme, Vec<ThemeIssue>), ThemeError> {
        let mut theme = Theme::from_toml_str(text)?;
        let issues = theme.validate();
        Ok((theme, issues))
    }

    /// Serialises to a complete TOML document.
    pub fn to_toml_string(&self) -> Result<String, ThemeError> {
        Ok(toml::to_string_pretty(self)?)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

/// Overlays `source` onto `target`; tables merge, everything else replaces.
fn merge_tables(target: &mut toml::Table, source: toml::Table) {
    for (key, value) in source {
        match (target.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(incoming)) => {
                merge_tables(inner, incoming);
            }
            (_, replacement) => {
                target.insert(key, replacement);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ButtonAlignment;

    #[test]
    fn partial_document_overrides_only_given_keys() {
        let theme = Theme::from_toml_str("[window]\ncorner_radius = 20.0\n").unwrap();
        assert_eq!(theme.window.corner_radius, 20.0);
        assert_eq!(theme.window.title_bar_height, 36.0);
        assert_eq!(theme.palette, Palette::dark());
    }

    #[test]
    fn mode_alone_selects_the_matching_builtin() {
        let theme = Theme::from_toml_str("mode = \"light\"").unwrap();
        assert_eq!(theme.palette, Palette::light());
        assert_eq!(theme.shadow, Shadow::for_mode(Mode::Light));
    }

    #[test]
    fn empty_document_is_the_dark_theme() {
        assert_eq!(Theme::from_toml_str("").unwrap(), Theme::dark());
    }

    #[test]
    fn nested_override_keeps_sibling_keys() {
        let doc = "[window.buttons]\nwidth = 40.0\n";
        let theme = Theme::from_toml_str(doc).unwrap();
        assert_eq!(theme.window.buttons.width, 40.0);
        assert_eq!(theme.window.buttons.height, 28.0);
        assert_eq!(theme.window.buttons.spacing, 2.0);
    }

    #[test]
    fn button_alignment_is_a_kebab_case_setting() {
        let theme = Theme::from_toml_str("[window.buttons]\nalignment = \"left\"\n").unwrap();
        assert_eq!(theme.window.buttons.alignment, ButtonAlignment::Left);
        assert_eq!(
            Theme::dark().window.buttons.alignment,
            ButtonAlignment::Right
        );
    }

    #[test]
    fn malformed_documents_are_errors_not_panics() {
        assert!(Theme::from_toml_str("[palette]\naccent = \"red\"").is_err());
        assert!(Theme::from_toml_str("mode = ").is_err());
        assert!(Theme::from_toml_str("mode = \"sepia\"").is_err());
    }

    #[test]
    fn full_round_trip_is_lossless() {
        for theme in [Theme::dark(), Theme::light()] {
            let text = theme.to_toml_string().unwrap();
            assert_eq!(Theme::from_toml_str(&text).unwrap(), theme);
        }
    }

    #[test]
    fn validated_load_repairs_bad_values() {
        let doc = "[window]\ncorner_radius = -5.0\n";
        let (theme, issues) = Theme::from_toml_str_validated(doc).unwrap();
        assert_eq!(theme.window.corner_radius, 0.0);
        assert_eq!(issues.len(), 1);
    }
}
