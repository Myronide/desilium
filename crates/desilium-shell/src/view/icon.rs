//! Application icons as HTML: the icon the application ships, found through the icon theme.
//!
//! Nothing is drawn on the application's behalf. An application without a resolvable icon gets
//! the theme's generic application icon, and only when the theme has not even that, a neutral
//! outline glyph.

use std::path::Path;

use desilium_apps::{IconResolver, IconSource, icons::FALLBACK_ICON};
use desilium_ui::icon_svg;

use super::esc;

/// The outline glyph of last resort.
const LAST_RESORT_GLYPH: &str = "grid";
/// Largest icon box in which a glyph fills the whole box.
const SMALL_ICON_PX: u32 = 24;

/// What to draw for an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IconSpec {
    /// An icon file (SVG or PNG) rendered through `<img>`.
    File(String),
    /// A neutral outline glyph from the shell's sprite, when no icon file exists at all.
    Glyph(&'static str),
}

impl IconSpec {
    /// The outline glyph of last resort.
    pub(crate) const NEUTRAL: IconSpec = IconSpec::Glyph(LAST_RESORT_GLYPH);
}

/// Percent-encodes a path into a `file://` URL.
pub(crate) fn file_url(path: &Path) -> String {
    let mut url = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        let plain = byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte);
        if plain {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

/// Icon names to try for an application, best first: the icon its desktop file or window names,
/// then names derived from its desktop id or app id (`org.kde.Dolphin.desktop` also tries
/// `org.kde.Dolphin`, `org.kde.dolphin` and `dolphin`).
pub(crate) fn candidate_names(icon: Option<&str>, id: &str) -> Vec<String> {
    let stem = id.strip_suffix(".desktop").unwrap_or(id);
    let last = stem.rsplit('.').next().unwrap_or(stem);
    let mut names: Vec<String> = Vec::new();
    for name in [
        icon.unwrap_or_default(),
        stem,
        &stem.to_ascii_lowercase(),
        &last.to_ascii_lowercase(),
    ] {
        if !name.is_empty() && !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
    }
    names
}

/// The first name the icon theme has; the theme's generic application icon if none; the neutral
/// glyph if even that is missing.
pub(crate) fn resolve_icon(
    resolver: &mut IconResolver,
    names: &[String],
    px: u32,
    scale: u32,
) -> IconSpec {
    let tried = names
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(FALLBACK_ICON));
    for name in tried {
        if let IconSource::Svg(path) | IconSource::Raster(path, _) =
            resolver.lookup(name, px, scale)
        {
            return IconSpec::File(file_url(&path));
        }
    }
    IconSpec::NEUTRAL
}

/// HTML for an icon; `size` is a design size class (`sm`, `md`, `lg`) or empty for the CSS default.
pub(crate) fn icon_html(spec: &IconSpec, size: &str, px: u32) -> String {
    match spec {
        IconSpec::File(url) => format!(
            "<img class=\"app-img {size}\" style=\"width:{px}px;height:{px}px\" src=\"{}\">",
            esc(url)
        ),
        IconSpec::Glyph(name) => {
            // Small glyphs (the bar) fill their box; big ones (dock, launcher) sit inside it.
            let glyph_px = if px <= SMALL_ICON_PX { px } else { px * 3 / 5 };
            format!(
                "<span class=\"app-glyph {size}\" style=\"width:{px}px;height:{px}px\">{}</span>",
                icon_svg(name, glyph_px, "")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_percent_encoded() {
        assert_eq!(
            file_url(Path::new("/usr/share/icons/a b/c#d.svg")),
            "file:///usr/share/icons/a%20b/c%23d.svg"
        );
    }

    #[test]
    fn candidates_start_with_the_icon_the_application_names() {
        assert_eq!(
            candidate_names(Some("kitty"), "kitty.desktop"),
            ["kitty"],
            "duplicates are dropped"
        );
        assert_eq!(
            candidate_names(Some("utilities-terminal"), "org.kde.Konsole.desktop"),
            [
                "utilities-terminal",
                "org.kde.Konsole",
                "org.kde.konsole",
                "konsole"
            ]
        );
        assert_eq!(candidate_names(None, ""), Vec::<String>::new());
    }

    #[test]
    fn a_window_without_a_desktop_file_is_tried_by_its_app_id() {
        assert_eq!(candidate_names(None, "Firefox"), ["Firefox", "firefox"]);
    }

    #[test]
    fn file_html_is_an_img() {
        let html = icon_html(&IconSpec::File("file:///x.png".into()), "md", 32);
        assert!(html.starts_with("<img"));
        assert!(html.contains("src=\"file:///x.png\""));
        assert!(html.contains("width:32px"));
    }

    #[test]
    fn the_neutral_glyph_is_an_outline_without_a_coloured_tile() {
        let html = icon_html(&IconSpec::NEUTRAL, "lg", 56);
        assert!(html.contains("app-glyph lg"));
        assert!(html.contains("<svg"));
        assert!(!html.contains("app-icon"), "no gradient tile");
    }
}
