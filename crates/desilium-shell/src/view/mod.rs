//! Shared HTML building blocks: escaping, style sheets and application icons.

pub mod icon;

/// Escapes text for HTML content and double-quoted attributes.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Style sheets shared by all surfaces (design tokens come from the theme, not from here).
pub const BASE_CSS: &str = include_str!("../../assets/base.css");
/// Top bar sheet.
pub const BAR_CSS: &str = include_str!("../../assets/bar.css");
/// Dock sheet.
pub const DOCK_CSS: &str = include_str!("../../assets/dock.css");
/// Launcher sheet.
pub const LAUNCHER_CSS: &str = include_str!("../../assets/launcher.css");
/// Tooltip sheet.
pub const TOOLTIP_CSS: &str = include_str!("../../assets/tooltip.css");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_quotes() {
        assert_eq!(
            esc("<b>\"a\" & 'b'</b>"),
            "&lt;b&gt;&quot;a&quot; &amp; &#39;b&#39;&lt;/b&gt;"
        );
        assert_eq!(esc("plain"), "plain");
    }
}
