//! Application icons as HTML: a real icon file when the theme has one, else the design's tile.

use std::path::Path;

use desilium_apps::{IconResolver, IconSource};
use desilium_ui::icon_svg;

use super::esc;
use crate::model::icons::{Tile, tile_for};

/// What to draw for an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconSpec {
    /// An icon file (SVG or PNG) rendered through `<img>`.
    File(String),
    /// Gradient squircle with a line glyph; never blank.
    Tile(Tile),
}

/// Percent-encodes a path into a `file://` URL.
pub fn file_url(path: &Path) -> String {
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

/// Resolves the icon of an application, falling back to the category tile.
pub fn resolve_icon(
    resolver: &mut IconResolver,
    desktop_id: &str,
    icon_name: Option<&str>,
    categories: &[String],
    px: u32,
    scale: u32,
) -> IconSpec {
    let found = icon_name.map(|name| resolver.lookup(name, px, scale));
    match found {
        Some(IconSource::Svg(path)) | Some(IconSource::Raster(path, _)) => {
            IconSpec::File(file_url(&path))
        }
        _ => IconSpec::Tile(tile_for(desktop_id, categories)),
    }
}

/// HTML for an icon; `size` is a design size class (`sm`, `md`, `lg`) or empty for the CSS default.
pub fn icon_html(spec: &IconSpec, size: &str, px: u32) -> String {
    match spec {
        IconSpec::File(url) => format!(
            "<img class=\"app-img {size}\" style=\"width:{px}px;height:{px}px\" src=\"{}\">",
            esc(url)
        ),
        IconSpec::Tile(tile) => format!(
            "<span class=\"app-icon a-{} {size}\">{}</span>",
            tile.gradient,
            icon_svg(tile.glyph, 24, "")
        ),
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
    fn tile_html_has_gradient_and_glyph() {
        let tile = IconSpec::Tile(tile_for("foot.desktop", &[]));
        let html = icon_html(&tile, "lg", 56);
        assert!(html.contains("app-icon a-terminal lg"));
        assert!(html.contains("<svg"));
    }

    #[test]
    fn file_html_is_an_img() {
        let html = icon_html(&IconSpec::File("file:///x.png".into()), "md", 32);
        assert!(html.starts_with("<img"));
        assert!(html.contains("src=\"file:///x.png\""));
        assert!(html.contains("width:32px"));
    }
}
