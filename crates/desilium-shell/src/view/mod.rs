//! Shared HTML building blocks: escaping, style sheets and application icons.

pub(crate) mod icon;

/// Escapes text for HTML content and double-quoted attributes.
pub(crate) fn esc(text: &str) -> String {
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

/// Logs a failed document edit. A missing element only means the surface is not built yet; the
/// next full draw sets the same state, so the edit is not worth more than a debug line.
pub(crate) fn note_failed(edit: Result<(), desilium_ui::UiError>, what: &str) {
    if let Err(error) = edit {
        tracing::debug!(%error, what, "document edit skipped");
    }
}

/// Style sheets shared by all surfaces (design tokens come from the theme, not from here).
pub(crate) const BASE_CSS: &str = include_str!("../../assets/base.css");
/// Top bar sheet.
pub(crate) const BAR_CSS: &str = include_str!("../../assets/bar.css");
/// Dock sheet.
pub(crate) const DOCK_CSS: &str = include_str!("../../assets/dock.css");
/// Launcher sheet.
pub(crate) const LAUNCHER_CSS: &str = include_str!("../../assets/launcher.css");
/// Tooltip sheet.
pub(crate) const TOOLTIP_CSS: &str = include_str!("../../assets/tooltip.css");

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
