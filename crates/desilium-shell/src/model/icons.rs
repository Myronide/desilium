//! Fallback tile (gradient squircle plus line glyph) chosen from an app's categories.
//!
//! Used whenever the icon theme has no usable file, so the dock never shows a blank.

/// A gradient tile of the design language (`.a-<gradient>`) and the glyph drawn on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Gradient name, matching the `--app-<name>` token.
    pub gradient: &'static str,
    /// Line icon name from the sprite.
    pub glyph: &'static str,
}

const TERMINAL_WORDS: [&str; 7] = [
    "terminal",
    "foot",
    "kitty",
    "konsole",
    "alacritty",
    "xterm",
    "wezterm",
];
const FILE_WORDS: [&str; 5] = ["nautilus", "dolphin", "thunar", "nemo", "pcmanfm"];

/// Ordered rules: the first rule with a matching category wins.
const CATEGORY_RULES: [(&[&str], Tile); 11] = [
    (
        &["TerminalEmulator"],
        Tile {
            gradient: "terminal",
            glyph: "terminal",
        },
    ),
    (
        &["FileManager", "FileTools"],
        Tile {
            gradient: "files",
            glyph: "folder",
        },
    ),
    (
        &["WebBrowser"],
        Tile {
            gradient: "web",
            glyph: "globe",
        },
    ),
    (
        &["Email", "InstantMessaging", "Chat"],
        Tile {
            gradient: "mail",
            glyph: "mail",
        },
    ),
    (
        &["Settings", "DesktopSettings", "PackageManager"],
        Tile {
            gradient: "settings",
            glyph: "settings",
        },
    ),
    (
        &["Calculator"],
        Tile {
            gradient: "calc",
            glyph: "calculator",
        },
    ),
    (
        &["TextEditor", "IDE", "Development"],
        Tile {
            gradient: "code",
            glyph: "code",
        },
    ),
    (
        &["Audio", "Music", "Player", "Video", "AudioVideo"],
        Tile {
            gradient: "music",
            glyph: "music",
        },
    ),
    (
        &["Graphics", "Photography", "Viewer"],
        Tile {
            gradient: "photos",
            glyph: "image",
        },
    ),
    (
        &["Calendar"],
        Tile {
            gradient: "calendar",
            glyph: "calendar",
        },
    ),
    (
        &["Office", "WordProcessor"],
        Tile {
            gradient: "notes",
            glyph: "file",
        },
    ),
];

/// Tile used when nothing matches.
pub const GENERIC_TILE: Tile = Tile {
    gradient: "store",
    glyph: "sparkles",
};

/// Picks the tile for an app from its desktop id stem and categories.
pub fn tile_for(desktop_id: &str, categories: &[String]) -> Tile {
    let id = desktop_id.to_ascii_lowercase();
    if TERMINAL_WORDS.iter().any(|word| id.contains(word)) {
        return CATEGORY_RULES[0].1;
    }
    if FILE_WORDS.iter().any(|word| id.contains(word)) {
        return CATEGORY_RULES[1].1;
    }
    if id.contains("settings") {
        return CATEGORY_RULES[4].1;
    }
    for (names, tile) in CATEGORY_RULES {
        if categories.iter().any(|c| names.contains(&c.as_str())) {
            return tile;
        }
    }
    GENERIC_TILE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn id_words_beat_categories() {
        assert_eq!(
            tile_for("foot.desktop", &cats(&["System"])).gradient,
            "terminal"
        );
        assert_eq!(tile_for("desilium-settings.desktop", &[]).glyph, "settings");
        assert_eq!(tile_for("org.kde.dolphin.desktop", &[]).gradient, "files");
    }

    #[test]
    fn categories_pick_the_tile() {
        assert_eq!(
            tile_for("x.desktop", &cats(&["Network", "WebBrowser"])).gradient,
            "web"
        );
        assert_eq!(
            tile_for("x.desktop", &cats(&["Development", "IDE"])).glyph,
            "code"
        );
        assert_eq!(
            tile_for("x.desktop", &cats(&["AudioVideo"])).gradient,
            "music"
        );
    }

    #[test]
    fn unknown_apps_get_the_generic_tile() {
        assert_eq!(tile_for("mystery.desktop", &[]), GENERIC_TILE);
    }

    #[test]
    fn every_glyph_exists_in_the_sprite() {
        for (_, tile) in CATEGORY_RULES.iter().chain([&(&[][..], GENERIC_TILE)]) {
            assert!(desilium_ui::has_icon(tile.glyph), "{}", tile.glyph);
        }
    }
}
