//! The Desilium theme as a KDE colour scheme, so Qt and KDE applications match the shell.
//!
//! KDE applications read their colours from the `[Colors:*]` groups of `kdeglobals`; a scheme
//! file is only the packaged form of the same keys, so the groups are written directly.

use desilium_theme::{Color, Mode, Palette, Theme};

use super::entry::ConfigEntry;

const FILE: &str = "kdeglobals";

/// `kdeglobals` entries that make KDE applications use `theme`.
pub fn scheme_entries(theme: &Theme) -> Vec<ConfigEntry> {
    let palette = &theme.palette;
    let name = match theme.mode {
        Mode::Dark => "DesiliumDark",
        Mode::Light => "DesiliumLight",
    };
    let mut entries = vec![
        ConfigEntry::text(FILE, "General", "ColorScheme", name),
        ConfigEntry::text(FILE, "General", "AccentColor", rgb(palette.accent)),
    ];
    for (group, set) in groups(palette) {
        entries.extend(set.entries(group));
    }
    entries.extend(window_manager_entries(palette));
    entries
}

/// The colours of one `[Colors:*]` group.
struct GroupColors {
    background: Color,
    alternate: Color,
    foreground: Color,
    inactive: Color,
    active: Color,
    link: Color,
    visited: Color,
    palette: Palette,
}

impl GroupColors {
    fn new(palette: &Palette, background: Color, alternate: Color) -> Self {
        Self {
            background,
            alternate,
            foreground: palette.text_primary,
            inactive: palette.text_secondary,
            active: palette.accent,
            link: palette.accent,
            visited: palette.accent_pressed,
            palette: palette.clone(),
        }
    }

    /// Text on the accent colour, used for selected items.
    fn on_accent(mut self) -> Self {
        let text = self.palette.text_on_accent;
        self.foreground = text;
        self.inactive = text;
        self.active = text;
        self.link = text;
        self.visited = text;
        self
    }

    fn entries(&self, group: &str) -> Vec<ConfigEntry> {
        let group = format!("Colors:{group}");
        let palette = &self.palette;
        let pairs = [
            ("BackgroundNormal", self.background),
            ("BackgroundAlternate", self.alternate),
            ("DecorationFocus", palette.accent),
            ("DecorationHover", palette.accent_hover),
            ("ForegroundNormal", self.foreground),
            ("ForegroundInactive", self.inactive),
            ("ForegroundActive", self.active),
            ("ForegroundLink", self.link),
            ("ForegroundVisited", self.visited),
            ("ForegroundNegative", palette.danger),
            ("ForegroundNeutral", palette.warning),
            ("ForegroundPositive", palette.success),
        ];
        pairs
            .into_iter()
            .map(|(key, color)| ConfigEntry::text(FILE, &group, key, rgb(color)))
            .collect()
    }
}

fn groups(palette: &Palette) -> Vec<(&'static str, GroupColors)> {
    let plain = |background, alternate| GroupColors::new(palette, background, alternate);
    vec![
        ("View", plain(palette.surface, palette.surface_raised)),
        ("Window", plain(palette.surface, palette.surface_raised)),
        ("Button", plain(palette.surface_raised, palette.overlay)),
        ("Tooltip", plain(palette.overlay, palette.surface_raised)),
        ("Complementary", plain(palette.background, palette.surface)),
        ("Header", plain(palette.surface_raised, palette.surface)),
        (
            "Selection",
            plain(palette.accent, palette.accent_hover).on_accent(),
        ),
    ]
}

/// Title bar colours for decorations that follow the scheme.
fn window_manager_entries(palette: &Palette) -> Vec<ConfigEntry> {
    [
        ("activeBackground", palette.surface_raised),
        ("activeForeground", palette.text_primary),
        ("inactiveBackground", palette.surface),
        ("inactiveForeground", palette.text_secondary),
    ]
    .into_iter()
    .map(|(key, color)| ConfigEntry::text(FILE, "WM", key, rgb(color)))
    .collect()
}

/// `R,G,B` in decimal, the notation KConfig uses for colours. Alpha is dropped.
fn rgb(color: Color) -> String {
    let [r, g, b, _] = color.to_bytes();
    format!("{r},{g},{b}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(entries: &'a [ConfigEntry], group: &str, key: &str) -> Option<&'a str> {
        entries
            .iter()
            .find(|entry| entry.id.group == group && entry.id.key == key)
            .map(|entry| entry.value.render())
    }

    #[test]
    fn colours_use_the_decimal_triplet_notation() {
        assert_eq!(rgb(Color::rgb(0x7B61FF)), "123,97,255");
        assert_eq!(rgb(Color::rgba(0x7B61FF29)), "123,97,255");
    }

    #[test]
    fn dark_theme_writes_scheme_name_accent_and_surfaces() {
        let theme = Theme::dark();
        let entries = scheme_entries(&theme);
        assert_eq!(
            value(&entries, "General", "ColorScheme"),
            Some("DesiliumDark")
        );
        assert_eq!(
            value(&entries, "General", "AccentColor"),
            Some("123,97,255")
        );
        assert_eq!(
            value(&entries, "Colors:View", "BackgroundNormal"),
            Some("21,24,33")
        );
        assert_eq!(
            value(&entries, "Colors:View", "ForegroundNormal"),
            Some("236,238,245")
        );
    }

    #[test]
    fn light_theme_has_its_own_scheme_name() {
        let entries = scheme_entries(&Theme::light());
        assert_eq!(
            value(&entries, "General", "ColorScheme"),
            Some("DesiliumLight")
        );
    }

    #[test]
    fn selection_uses_text_on_accent_everywhere() {
        let theme = Theme::dark();
        let entries = scheme_entries(&theme);
        let on_accent = rgb(theme.palette.text_on_accent);
        for key in ["ForegroundNormal", "ForegroundActive", "ForegroundLink"] {
            assert_eq!(
                value(&entries, "Colors:Selection", key),
                Some(on_accent.as_str()),
                "{key}"
            );
        }
        assert_eq!(
            value(&entries, "Colors:Selection", "BackgroundNormal"),
            Some("123,97,255")
        );
    }

    #[test]
    fn every_group_has_all_twelve_keys_and_ids_are_unique() {
        let entries = scheme_entries(&Theme::dark());
        let colour_groups = 7;
        let keys_per_group = 12;
        let expected = 2 + colour_groups * keys_per_group + 4;
        assert_eq!(entries.len(), expected);
        let mut ids: Vec<_> = entries.iter().map(|entry| entry.id.clone()).collect();
        ids.sort_by(|a, b| (&a.group, &a.key).cmp(&(&b.group, &b.key)));
        ids.dedup();
        assert_eq!(ids.len(), expected, "no entry is written twice");
    }

    #[test]
    fn title_bar_colours_follow_the_palette() {
        let theme = Theme::dark();
        let entries = scheme_entries(&theme);
        assert_eq!(
            value(&entries, "WM", "activeBackground"),
            Some(rgb(theme.palette.surface_raised).as_str())
        );
    }
}
