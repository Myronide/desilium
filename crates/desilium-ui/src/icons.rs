//! Icon table built from the design sprite, emitted as self-contained inline SVG.
//!
//! Blitz cannot resolve `<use href="#id">` against a sprite elsewhere in the
//! document (each inline `<svg>` renders in isolation), so every icon is
//! expanded to its full path data instead. The sprite in `assets/icons.svg` is a
//! copy of `docs/design/icons.svg`; replace it there and here together.

use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
};

const SPRITE: &str = include_str!("../assets/icons.svg");
const SYMBOL_PREFIX: &str = "<symbol id=\"i-";
const SYMBOL_END: &str = "</symbol>";
/// Shown for unknown names: a dashed circle with a question mark.
const PLACEHOLDER_BODY: &str = "<circle cx=\"12\" cy=\"12\" r=\"8.5\" stroke-dasharray=\"3 3\"/>\
    <path d=\"M9.8 9.6a2.3 2.3 0 1 1 3.4 2c-.8.5-1.2 1-1.2 1.9\"/><path d=\"M12 17h.01\" stroke-width=\"2.4\"/>";

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| parse_sprite(SPRITE))
}

fn parse_sprite(sprite: &'static str) -> HashMap<&'static str, &'static str> {
    let mut icons = HashMap::new();
    let mut rest = sprite;
    while let Some(start) = rest.find(SYMBOL_PREFIX) {
        let after = &rest[start + SYMBOL_PREFIX.len()..];
        let Some((name, tail)) = after.split_once('"') else {
            break;
        };
        let Some((_attrs, tail)) = tail.split_once('>') else {
            break;
        };
        let Some((body, tail)) = tail.split_once(SYMBOL_END) else {
            break;
        };
        icons.insert(name, body);
        rest = tail;
    }
    icons
}

/// True when the sprite contains an icon called `name` (without the `i-` prefix).
pub fn has_icon(name: &str) -> bool {
    table().contains_key(name)
}

/// All icon names, sorted.
pub fn icon_names() -> Vec<&'static str> {
    let mut names: Vec<_> = table().keys().copied().collect();
    names.sort_unstable();
    names
}

fn warn_once(name: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut seen) = seen.lock() else {
        return;
    };
    if seen.insert(name.to_owned()) {
        tracing::warn!(icon = name, "unknown icon, drawing a placeholder");
    }
}

fn clean_class(class: &str) -> String {
    class
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect()
}

/// Inline `<svg>` for icon `name` at `size` logical pixels with extra CSS `class`.
///
/// The result is self-contained (no `<use>`), strokes use `currentColor`, and
/// the element always carries the `ic` class so `shared.css` rules apply.
/// Unknown names yield a visible placeholder and one logged warning.
pub fn icon_svg(name: &str, size: u32, class: &str) -> String {
    let style = format!("style=\"width:{size}px;height:{size}px\"");
    icon_markup(name, size, class, &style)
}

/// Like [`icon_svg`] but sized only through `width`/`height` attributes, so
/// stylesheet rules (`.ic.s16`, `.app-icon .ic`) decide the final size.
fn icon_styled_by_css(name: &str, size: u32, class: &str) -> String {
    icon_markup(name, size, class, "")
}

fn icon_markup(name: &str, size: u32, class: &str, style: &str) -> String {
    let body = table().get(name).copied().unwrap_or_else(|| {
        warn_once(name);
        PLACEHOLDER_BODY
    });
    let class = clean_class(class);
    format!(
        "<svg class=\"ic {class}\" viewBox=\"0 0 24 24\" width=\"{size}\" height=\"{size}\" \
         {style} fill=\"none\" stroke=\"currentColor\" \
         stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{body}</svg>"
    )
}

const USE_MARKER: &str = "<use href=\"#i-";

fn class_of(svg_open: &str) -> &str {
    let Some(start) = svg_open.find("class=\"") else {
        return "";
    };
    let rest = &svg_open[start + "class=\"".len()..];
    rest.split('"').next().unwrap_or("")
}

fn size_class(class: &str) -> u32 {
    class
        .split_whitespace()
        .find_map(|token| token.strip_prefix('s').and_then(|n| n.parse().ok()))
        .unwrap_or(DEFAULT_ICON_SIZE)
}

/// Rewrites `<svg class="ic s16"><use href="#i-name"/></svg>` references (the
/// design mockups' sprite style) into self-contained [`icon_svg`] elements.
///
/// Lets the built mockups in `docs/design/*.html` render in Blitz unmodified.
pub fn inline_sprite_uses(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(use_at) = rest.find(USE_MARKER) {
        let before = &rest[..use_at];
        let Some(svg_at) = before.rfind("<svg") else {
            break;
        };
        let after_use = &rest[use_at + USE_MARKER.len()..];
        let (Some((name, _)), Some(close)) = (after_use.split_once('"'), after_use.find("</svg>"))
        else {
            break;
        };
        let open_end = before[svg_at..]
            .find('>')
            .map_or(before.len(), |i| svg_at + i);
        let class = class_of(&before[svg_at..open_end]).replace("ic", "");
        out.push_str(&before[..svg_at]);
        out.push_str(&icon_styled_by_css(name, size_class(&class), &class));
        rest = &after_use[close + "</svg>".len()..];
    }
    out.push_str(rest);
    out
}

/// Pixel size used when a mockup icon carries no `sNN` class (`--icon-md`).
const DEFAULT_ICON_SIZE: u32 = 20;

/// Glyph used inside each gradient app tile (same table as `docs/design/build.py`).
const APP_GLYPHS: [(&str, &str); 12] = [
    ("files", "folder"),
    ("terminal", "terminal"),
    ("web", "globe"),
    ("settings", "settings"),
    ("music", "music"),
    ("photos", "image"),
    ("code", "code"),
    ("calc", "calculator"),
    ("mail", "mail"),
    ("notes", "file"),
    ("calendar", "calendar"),
    ("store", "sparkles"),
];

/// Gradient app-icon tile (`.app-icon.a-<name>`) with its glyph; `class` adds size classes like `lg`.
///
/// Unknown app names fall back to a generic tile with the placeholder glyph.
pub fn app_icon_html(app: &str, class: &str) -> String {
    let glyph = APP_GLYPHS
        .iter()
        .find(|(name, _)| *name == app)
        .map_or("", |(_, glyph)| *glyph);
    let app = clean_class(app);
    let class = clean_class(class);
    let svg = icon_svg(glyph, 24, "");
    format!("<span class=\"app-icon a-{app} {class}\">{svg}</span>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_is_parsed() {
        assert!(icon_names().len() > 40);
        assert!(has_icon("search"));
        assert!(!has_icon("nonsense"));
    }

    #[test]
    fn output_is_self_contained() {
        let svg = icon_svg("wifi", 20, "s20 muted");
        assert!(svg.starts_with("<svg class=\"ic s20 muted\""));
        assert!(svg.contains("viewBox=\"0 0 24 24\""));
        assert!(svg.contains("currentColor"));
        assert!(!svg.contains("<use"));
    }

    #[test]
    fn unknown_icon_gets_a_placeholder() {
        let svg = icon_svg("no-such-icon", 16, "");
        assert!(svg.contains("stroke-dasharray"));
    }

    #[test]
    fn app_tiles_use_existing_glyphs() {
        for (_, glyph) in APP_GLYPHS {
            assert!(has_icon(glyph), "{glyph}");
        }
        assert!(app_icon_html("files", "lg").contains("a-files lg"));
    }

    #[test]
    fn sprite_references_are_inlined() {
        let html = "<p><svg class=\"ic s16\"><use href=\"#i-wifi\"/></svg>x<svg class=\"ic\"><use href=\"#i-bell\"/></svg></p>";
        let out = inline_sprite_uses(html);
        assert!(!out.contains("<use"), "{out}");
        assert_eq!(out.matches("<svg").count(), 2);
        assert!(out.contains("width=\"16\""));
        assert!(out.contains("width=\"20\""));
        assert!(out.ends_with("</svg></p>"));
    }

    #[test]
    fn class_is_sanitised() {
        assert!(!icon_svg("wifi", 16, "a\"onload=x").contains("onload=x\""));
    }

    #[test]
    fn every_icon_used_in_the_design_sources_exists() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/src");
        let mut used = HashSet::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for chunk in text.split("{{").skip(1) {
                let Some((inner, _)) = chunk.split_once("}}") else {
                    continue;
                };
                let name = inner.split('|').next().unwrap().trim();
                if name != "sprite"
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    used.insert(name.to_owned());
                }
            }
        }
        assert!(used.len() > 30, "found only {} icon references", used.len());
        let missing: Vec<_> = used.iter().filter(|n| !has_icon(n)).collect();
        assert!(missing.is_empty(), "missing icons: {missing:?}");
    }
}
