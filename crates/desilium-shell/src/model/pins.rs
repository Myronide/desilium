//! The persistent list of pinned dock apps (`dock.toml`) and its first-run default.

use serde::{Deserialize, Serialize};

/// Schema version written to `dock.toml`.
pub const DOCK_FILE_VERSION: u32 = 1;
/// File name inside the Desilium config directory.
pub const DOCK_FILE_NAME: &str = "dock.toml";

/// Well-known desktop ids per dock slot; the first installed one of each slot is pinned.
///
/// Order is the order in the dock: files, browser, editor, terminal, settings.
const SLOT_CANDIDATES: [&[&str]; 5] = [
    &[
        "org.desilium.Doria.desktop",
        "org.gnome.Nautilus.desktop",
        "org.kde.dolphin.desktop",
        "thunar.desktop",
        "nemo.desktop",
        "pcmanfm-qt.desktop",
        "pcmanfm.desktop",
        "org.gnome.Files.desktop",
    ],
    &[
        "firefox.desktop",
        "org.mozilla.firefox.desktop",
        "librewolf.desktop",
        "chromium.desktop",
        "org.chromium.Chromium.desktop",
        "google-chrome.desktop",
        "brave-browser.desktop",
        "vivaldi-stable.desktop",
        "org.gnome.Epiphany.desktop",
    ],
    &[
        "code.desktop",
        "codium.desktop",
        "dev.zed.Zed.desktop",
        "org.kde.kate.desktop",
        "org.gnome.TextEditor.desktop",
        "org.gnome.gedit.desktop",
        "geany.desktop",
        "sublime_text.desktop",
    ],
    &[
        "kitty.desktop",
        "foot.desktop",
        "org.kde.konsole.desktop",
        "Alacritty.desktop",
        "alacritty.desktop",
        "org.gnome.Terminal.desktop",
        "xterm.desktop",
    ],
    &["desilium-settings.desktop"],
];

/// Contents of `dock.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    /// Schema version.
    pub version: u32,
    /// Pinned desktop file ids, in dock order.
    pub pinned: Vec<String>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            version: DOCK_FILE_VERSION,
            pinned: Vec::new(),
        }
    }
}

/// Errors reading or writing `dock.toml`.
#[derive(Debug, thiserror::Error)]
pub enum PinsError {
    /// The file is not valid TOML for [`DockConfig`].
    #[error("invalid dock.toml: {0}")]
    Parse(#[from] toml::de::Error),
    /// The config could not be serialised.
    #[error("cannot serialise dock.toml: {0}")]
    Render(#[from] toml::ser::Error),
}

impl DockConfig {
    /// Parses a file body. Duplicate and blank ids are dropped.
    pub fn parse(text: &str) -> Result<Self, PinsError> {
        let mut config: DockConfig = toml::from_str(text)?;
        let mut seen = Vec::new();
        config.pinned.retain(|id| {
            let fresh = !id.trim().is_empty() && !seen.contains(id);
            if fresh {
                seen.push(id.clone());
            }
            fresh
        });
        Ok(config)
    }

    /// Serialises to the file body.
    pub fn render(&self) -> Result<String, PinsError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// First-run default: the first installed candidate of every slot.
    pub fn first_run(is_installed: impl Fn(&str) -> bool) -> Self {
        let pinned = SLOT_CANDIDATES
            .iter()
            .filter_map(|slot| slot.iter().find(|id| is_installed(id)))
            .map(|id| (*id).to_owned())
            .collect();
        Self {
            version: DOCK_FILE_VERSION,
            pinned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_takes_the_first_installed_of_each_slot() {
        let installed = [
            "thunar.desktop",
            "nemo.desktop",
            "chromium.desktop",
            "kitty.desktop",
        ];
        let config = DockConfig::first_run(|id| installed.contains(&id));
        assert_eq!(
            config.pinned,
            ["thunar.desktop", "chromium.desktop", "kitty.desktop"]
        );
    }

    #[test]
    fn first_run_without_apps_pins_nothing() {
        assert!(DockConfig::first_run(|_| false).pinned.is_empty());
    }

    #[test]
    fn round_trip() {
        let config = DockConfig {
            version: DOCK_FILE_VERSION,
            pinned: vec!["a.desktop".into(), "b.desktop".into()],
        };
        let text = config.render().unwrap();
        assert_eq!(DockConfig::parse(&text).unwrap(), config);
    }

    #[test]
    fn parse_drops_duplicates_and_blanks_and_defaults_missing_fields() {
        let config =
            DockConfig::parse("pinned = ['a.desktop', '', 'a.desktop', 'b.desktop']").unwrap();
        assert_eq!(config.pinned, ["a.desktop", "b.desktop"]);
        assert_eq!(config.version, DOCK_FILE_VERSION);
        assert!(DockConfig::parse("").unwrap().pinned.is_empty());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(DockConfig::parse("pinned = 3").is_err());
        assert!(DockConfig::parse("[[[").is_err());
    }
}
