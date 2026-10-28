//! Reading the user's KDE theme: the colour scheme and the icon theme.
//!
//! KDE keeps both in `kdeglobals` (`[Colors:*]` groups, `[General] ColorScheme`, `[Icons] Theme`),
//! with system defaults in `XDG_CONFIG_DIRS` and packaged schemes in `color-schemes/*.colors`.
//! Parsing is pure; only [`KdeDirs`] and [`load_theme`] touch the file system, and a missing or
//! unusable KDE configuration is an error value, never a panic.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Color, Mode, Palette, Theme};

const KDEGLOBALS: &str = "kdeglobals";
const SCHEME_DIR: &str = "color-schemes";
const SCHEME_EXTENSION: &str = "colors";
const DEFAULT_CONFIG_DIRS: &str = "/etc/xdg";
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
/// Where dark and light surfaces meet: the relative luminance of the window colour.
const DARK_BELOW_LUMINANCE: f32 = 0.4;

/// Why the KDE theme could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KdeThemeError {
    /// Neither `kdeglobals` nor a named scheme file has the window colours.
    #[error("no KDE colour scheme found (no [Colors:Window] in kdeglobals or in a scheme file)")]
    NoScheme,
    /// A colour was not three or four comma separated numbers.
    #[error("malformed colour {value:?} for {group}/{key}")]
    BadColor {
        /// Group of the key.
        group: String,
        /// The key.
        key: String,
        /// Its value.
        value: String,
    },
}

/// Parsed `key=value` pairs by group; later layers never replace earlier ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KdeConfig {
    values: HashMap<(String, String), String>,
}

impl KdeConfig {
    /// Parses KConfig INI text. Comments, blank lines and `[Group][$i]`-style suffixes are tolerated.
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        config.add_layer(text);
        config
    }

    /// Adds `text` below what is already known: a key that is present keeps its value.
    pub fn add_layer(&mut self, text: &str) {
        let mut group = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[') {
                let name = header.split(']').next().unwrap_or_default();
                group = name.to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.split('[').next().unwrap_or(key).trim().to_owned();
            self.values
                .entry((group.clone(), key))
                .or_insert_with(|| value.trim().to_owned());
        }
    }

    /// The value of `key` in `group`.
    pub fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.values
            .get(&(group.to_owned(), key.to_owned()))
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// The icon theme name from `[Icons] Theme`.
    pub fn icon_theme(&self) -> Option<&str> {
        self.get("Icons", "Theme")
    }

    /// The scheme name from `[General] ColorScheme`.
    pub fn scheme_name(&self) -> Option<&str> {
        self.get("General", "ColorScheme")
    }

    fn color(&self, group: &str, key: &str) -> Result<Option<Color>, KdeThemeError> {
        let Some(value) = self.get(group, key) else {
            return Ok(None);
        };
        parse_color(value)
            .map(Some)
            .ok_or_else(|| KdeThemeError::BadColor {
                group: group.to_owned(),
                key: key.to_owned(),
                value: value.to_owned(),
            })
    }

    fn has_window_colours(&self) -> bool {
        self.get("Colors:Window", "BackgroundNormal").is_some()
    }

    /// A Desilium theme with this scheme's colours.
    pub fn to_theme(&self) -> Result<Theme, KdeThemeError> {
        if !self.has_window_colours() {
            return Err(KdeThemeError::NoScheme);
        }
        let colour = |group: &str, key: &str| self.color(&format!("Colors:{group}"), key);
        let window = colour("Window", "BackgroundNormal")?.ok_or(KdeThemeError::NoScheme)?;
        let mode = if window.relative_luminance() < DARK_BELOW_LUMINANCE {
            Mode::Dark
        } else {
            Mode::Light
        };
        let fallback = Palette::for_mode(mode);
        let or = |found: Option<Color>, default: Color| found.unwrap_or(default);

        let button = or(colour("Button", "BackgroundNormal")?, window);
        let tooltip = or(colour("Tooltip", "BackgroundNormal")?, button);
        let text = or(colour("Window", "ForegroundNormal")?, fallback.text_primary);
        let text_inactive = or(
            colour("Window", "ForegroundInactive")?,
            fallback.text_secondary,
        );
        let accent = or(colour("Selection", "BackgroundNormal")?, fallback.accent);
        let on_accent = or(
            colour("Selection", "ForegroundNormal")?,
            fallback.text_on_accent,
        );
        let toward_text = |amount: f32| window.mix(text, amount);

        let palette = Palette {
            background: match mode {
                Mode::Dark => window.darken(0.3),
                Mode::Light => window.darken(0.04),
            },
            surface: window,
            surface_raised: button,
            overlay: tooltip,
            text_primary: text,
            text_secondary: text_inactive,
            text_disabled: text_inactive.mix(window, 0.25),
            text_on_accent: on_accent,
            accent,
            accent_hover: accent.lighten(0.08),
            accent_pressed: accent.darken(0.08),
            accent_subtle: accent.with_alpha(0.16),
            success: or(colour("Window", "ForegroundPositive")?, fallback.success),
            warning: or(colour("Window", "ForegroundNeutral")?, fallback.warning),
            danger: or(colour("Window", "ForegroundNegative")?, fallback.danger),
            border: toward_text(0.14),
            divider: toward_text(0.08),
            selection: accent.with_alpha(0.32),
        };
        let mut theme = Theme::derive_from_accent(accent, mode);
        theme.name = "KDE colours".to_owned();
        theme.palette = palette;
        Ok(theme)
    }
}

/// `r,g,b` or `r,g,b,a` in decimal bytes, the notation KConfig uses.
fn parse_color(value: &str) -> Option<Color> {
    let parts: Vec<u8> = value
        .split(',')
        .map(|part| part.trim().parse::<u8>().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [r, g, b] => Some(Color::from_bytes(*r, *g, *b, u8::MAX)),
        [r, g, b, a] => Some(Color::from_bytes(*r, *g, *b, *a)),
        _ => None,
    }
}

/// The directories KDE configuration and schemes are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KdeDirs {
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config_home: PathBuf,
    /// `$XDG_CONFIG_DIRS`, system defaults after the user's file.
    pub config_dirs: Vec<PathBuf>,
    /// `$XDG_DATA_HOME` first, then `$XDG_DATA_DIRS`: where `color-schemes/` lives.
    pub data_dirs: Vec<PathBuf>,
}

impl KdeDirs {
    /// Reads the XDG variables of the running process.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds the directories from an injectable variable lookup.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| lookup(key).filter(|value| !value.is_empty());
        let home = get("HOME").map(PathBuf::from).unwrap_or_default();
        let absolute = |key: &str| {
            get(key)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let list = |key: &str, default: &str| -> Vec<PathBuf> {
            get(key)
                .unwrap_or_else(|| default.to_owned())
                .split(':')
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .collect()
        };
        let mut data_dirs =
            vec![absolute("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"))];
        data_dirs.extend(list("XDG_DATA_DIRS", DEFAULT_DATA_DIRS));
        Self {
            config_home: absolute("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            config_dirs: list("XDG_CONFIG_DIRS", DEFAULT_CONFIG_DIRS),
            data_dirs,
        }
    }

    /// Where the user's `kdeglobals` lives; changes there mean the theme changed.
    pub fn user_kdeglobals(&self) -> PathBuf {
        self.config_home.join(KDEGLOBALS)
    }

    /// `kdeglobals` of the user and the system, then the scheme file it names when the window
    /// colours are not spelled out in `kdeglobals` itself.
    pub fn load(&self) -> KdeConfig {
        let mut config = KdeConfig::default();
        let files = std::iter::once(self.config_home.join(KDEGLOBALS))
            .chain(self.config_dirs.iter().map(|dir| dir.join(KDEGLOBALS)));
        for file in files {
            if let Ok(text) = fs::read_to_string(&file) {
                config.add_layer(&text);
            }
        }
        if !config.has_window_colours()
            && let Some(name) = config.scheme_name().map(str::to_owned)
            && let Some(text) = self
                .scheme_file(&name)
                .and_then(|p| fs::read_to_string(p).ok())
        {
            config.add_layer(&text);
        }
        config
    }

    /// Only the scheme file `name` (`color-schemes/<name>.colors` below the data directories),
    /// for a scheme the user picked in the settings instead of the one `kdeglobals` names.
    pub fn load_scheme(&self, name: &str) -> Option<KdeConfig> {
        let text = fs::read_to_string(self.scheme_file(name)?).ok()?;
        Some(KdeConfig::parse(&text))
    }

    fn scheme_file(&self, name: &str) -> Option<PathBuf> {
        // The name comes from a user file and becomes a path component: no separators.
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        self.data_dirs
            .iter()
            .map(|dir| {
                dir.join(SCHEME_DIR)
                    .join(format!("{name}.{SCHEME_EXTENSION}"))
            })
            .find(|path| Path::new(path).is_file())
    }
}

/// The theme of the user's current KDE colour scheme.
pub fn load_theme() -> Result<Theme, KdeThemeError> {
    KdeDirs::from_env().load().to_theme()
}

/// The theme of the scheme the user picked (`Some(name)`), or of the one `kdeglobals` names.
pub fn load_theme_for(picked: Option<&str>) -> Result<Theme, KdeThemeError> {
    match picked {
        Some(name) => load_named_theme(name),
        None => load_theme(),
    }
}

/// The theme of the colour scheme file `name`; an unknown or unusable scheme is an error.
pub fn load_named_theme(name: &str) -> Result<Theme, KdeThemeError> {
    KdeDirs::from_env()
        .load_scheme(name)
        .ok_or(KdeThemeError::NoScheme)?
        .to_theme()
}

/// The icon theme named in the user's `kdeglobals`, if any.
pub fn icon_theme() -> Option<String> {
    KdeDirs::from_env().load().icon_theme().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BREEZE_DARK: &str = "\
[General]
ColorScheme=BreezeDark

[Icons]
Theme=breeze-dark

[Colors:Window]
BackgroundNormal=32,35,38
ForegroundNormal=252,252,252
ForegroundInactive=161,169,177
ForegroundPositive=39,174,96
ForegroundNeutral=246,116,0
ForegroundNegative=218,68,83

[Colors:Button]
BackgroundNormal=41,44,48

[Colors:Tooltip]
BackgroundNormal=49,54,59

[Colors:Selection]
BackgroundNormal=61,174,233
ForegroundNormal=252,252,252
";

    #[test]
    fn parses_groups_keys_and_the_icon_theme() {
        let config = KdeConfig::parse(BREEZE_DARK);
        assert_eq!(config.icon_theme(), Some("breeze-dark"));
        assert_eq!(config.scheme_name(), Some("BreezeDark"));
        assert_eq!(
            config.get("Colors:Button", "BackgroundNormal"),
            Some("41,44,48")
        );
        assert_eq!(config.get("Colors:Button", "Missing"), None);
    }

    #[test]
    fn earlier_layers_win_and_localised_keys_are_tolerated() {
        let mut config = KdeConfig::parse("[Icons]\nTheme=user\n# comment\n[Icons][$i]\n");
        config.add_layer("[Icons]\nTheme=system\n[General]\nName[de]=Wert\nName=Value\n");
        assert_eq!(config.icon_theme(), Some("user"));
        assert_eq!(
            config.get("General", "Name"),
            Some("Wert"),
            "first spelling wins"
        );
    }

    #[test]
    fn a_dark_scheme_becomes_a_dark_theme_with_its_colours() {
        let theme = KdeConfig::parse(BREEZE_DARK).to_theme().expect("theme");
        assert_eq!(theme.mode, Mode::Dark);
        assert_eq!(theme.palette.surface, Color::from_bytes(32, 35, 38, 255));
        assert_eq!(
            theme.palette.surface_raised,
            Color::from_bytes(41, 44, 48, 255)
        );
        assert_eq!(theme.palette.accent, Color::from_bytes(61, 174, 233, 255));
        assert_eq!(
            theme.palette.text_primary,
            Color::from_bytes(252, 252, 252, 255)
        );
        assert_eq!(theme.palette.danger, Color::from_bytes(218, 68, 83, 255));
        assert_eq!(theme.name, "KDE colours");
    }

    #[test]
    fn a_bright_scheme_becomes_a_light_theme() {
        let text = BREEZE_DARK.replace("32,35,38", "239,240,241").replace(
            "252,252,252\nForegroundInactive",
            "35,38,39\nForegroundInactive",
        );
        let theme = KdeConfig::parse(&text).to_theme().expect("theme");
        assert_eq!(theme.mode, Mode::Light);
        assert!(
            theme.palette.background.relative_luminance()
                < theme.palette.surface.relative_luminance()
        );
    }

    #[test]
    fn missing_colours_are_an_error_not_a_default() {
        assert_eq!(
            KdeConfig::parse("[Icons]\nTheme=x\n").to_theme(),
            Err(KdeThemeError::NoScheme)
        );
    }

    #[test]
    fn a_malformed_colour_is_reported_with_its_place() {
        let text = BREEZE_DARK.replace("61,174,233", "blue");
        let Err(KdeThemeError::BadColor { group, key, value }) = KdeConfig::parse(&text).to_theme()
        else {
            panic!("expected a bad colour");
        };
        assert_eq!(
            (group.as_str(), key.as_str(), value.as_str()),
            ("Colors:Selection", "BackgroundNormal", "blue")
        );
    }

    #[test]
    fn colours_accept_an_alpha_channel_and_reject_out_of_range_numbers() {
        assert_eq!(parse_color("1,2,3,4"), Some(Color::from_bytes(1, 2, 3, 4)));
        assert_eq!(parse_color("256,0,0"), None);
        assert_eq!(parse_color("1,2"), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("desilium-kde-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn dirs_in(root: &Path) -> KdeDirs {
        KdeDirs {
            config_home: root.join("config"),
            config_dirs: vec![root.join("etc")],
            data_dirs: vec![root.join("data")],
        }
    }

    #[test]
    fn the_scheme_file_named_in_kdeglobals_is_used_when_kdeglobals_has_no_colours() {
        let root = scratch("scheme-file");
        fs::create_dir_all(root.join("config")).expect("dir");
        fs::create_dir_all(root.join("data/color-schemes")).expect("dir");
        fs::write(
            root.join("config/kdeglobals"),
            "[General]\nColorScheme=BreezeDark\n[Icons]\nTheme=Papirus\n",
        )
        .expect("write");
        fs::write(
            root.join("data/color-schemes/BreezeDark.colors"),
            BREEZE_DARK,
        )
        .expect("write");
        let config = dirs_in(&root).load();
        assert_eq!(
            config.icon_theme(),
            Some("Papirus"),
            "the user's icon theme wins"
        );
        assert_eq!(config.to_theme().expect("theme").mode, Mode::Dark);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn kdeglobals_colours_win_over_the_scheme_file_and_system_files_fill_gaps() {
        let root = scratch("layers");
        fs::create_dir_all(root.join("config")).expect("dir");
        fs::create_dir_all(root.join("etc")).expect("dir");
        fs::write(root.join("config/kdeglobals"), BREEZE_DARK).expect("write");
        fs::write(
            root.join("etc/kdeglobals"),
            "[Icons]\nTheme=breeze\n[KDE]\nWidgetStyle=Breeze\n",
        )
        .expect("write");
        let config = dirs_in(&root).load();
        assert_eq!(config.icon_theme(), Some("breeze-dark"));
        assert_eq!(config.get("KDE", "WidgetStyle"), Some("Breeze"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_picked_scheme_is_read_alone_and_unknown_ones_are_none() {
        let root = scratch("picked");
        fs::create_dir_all(root.join("config")).expect("dir");
        fs::create_dir_all(root.join("data/color-schemes")).expect("dir");
        // kdeglobals names another scheme and has colours of its own; the pick must not mix.
        fs::write(root.join("config/kdeglobals"), BREEZE_DARK).expect("write");
        let light =
            BREEZE_DARK.replace("BackgroundNormal=32,35,38", "BackgroundNormal=250,250,250");
        fs::write(root.join("data/color-schemes/Daylight.colors"), light).expect("write");
        let dirs = dirs_in(&root);
        let theme = dirs
            .load_scheme("Daylight")
            .expect("the file exists")
            .to_theme()
            .expect("theme");
        assert_eq!(theme.mode, Mode::Light);
        assert!(dirs.load_scheme("Missing").is_none());
        assert!(dirs.load_scheme("../config/kdeglobals").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scheme_names_cannot_leave_the_scheme_directory() {
        let dirs = dirs_in(Path::new("/nonexistent"));
        assert_eq!(dirs.scheme_file("../../etc/passwd"), None);
        assert_eq!(dirs.scheme_file(".hidden"), None);
        assert_eq!(dirs.scheme_file(""), None);
    }

    #[test]
    fn no_files_at_all_is_just_no_scheme() {
        let root = scratch("empty");
        let dirs = dirs_in(&root);
        assert_eq!(dirs.load().to_theme(), Err(KdeThemeError::NoScheme));
        assert_eq!(dirs.load().icon_theme(), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn directories_follow_the_xdg_variables_and_defaults() {
        let env = |key: &str| match key {
            "HOME" => Some("/home/u".to_owned()),
            "XDG_CONFIG_HOME" => Some("relative/ignored".to_owned()),
            "XDG_DATA_DIRS" => Some("/opt/share:/usr/share".to_owned()),
            _ => None,
        };
        let dirs = KdeDirs::from_lookup(env);
        assert_eq!(
            dirs.config_home,
            PathBuf::from("/home/u/.config"),
            "relative values are ignored"
        );
        assert_eq!(dirs.config_dirs, vec![PathBuf::from("/etc/xdg")]);
        assert_eq!(
            dirs.data_dirs,
            vec![
                PathBuf::from("/home/u/.local/share"),
                PathBuf::from("/opt/share"),
                PathBuf::from("/usr/share")
            ]
        );
        assert_eq!(
            dirs.user_kdeglobals(),
            PathBuf::from("/home/u/.config/kdeglobals")
        );
    }
}
