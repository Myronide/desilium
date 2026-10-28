//! HTML generation. Pure functions from (model, configuration, theme) to markup.
//!
//! The document has three regions the app updates independently: `#nav` (sidebar
//! entries), `#main` (header, banner and page) and, inside the page, `#toast`.
//! The search field lives outside all of them so typing never loses focus.

mod about;
mod controls;
pub use controls::range_updates;
mod keyboard;
mod page;
mod preview;
mod search_page;
mod themes;

use std::{fmt::Write, path::Path, time::Instant};

use desilium_config_types::DesiliumConfig;
use desilium_theme::Theme;
use desilium_ui::icon_svg;

use crate::{
    changes::Recovery,
    i18n::{keys, tr},
    model::{Level, Model, Notice, Toast},
    registry::{NAV, NavEntry},
};

/// Component styles of the design language (copy of `docs/design/shared.css`).
pub const SHARED_CSS: &str = include_str!("../../assets/shared.css");
/// Styles of the settings app.
pub const SETTINGS_CSS: &str = include_str!("../../assets/settings.css");

/// English tooltip of entries that are planned but not built yet.
const PLANNED_TOOLTIP: &str = "Coming soon";

/// Everything a view function may read.
pub struct Ctx<'a> {
    /// View state.
    pub model: &'a Model,
    /// The active configuration.
    pub config: &'a DesiliumConfig,
    /// The theme derived from it.
    pub theme: &'a Theme,
    /// Current time, for highlight expiry.
    pub now: Instant,
}

/// Escapes text for element content and attribute values.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// `file://` URL for a local path, percent-encoding everything but unreserved characters.
pub fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Selector of the wrapper that carries the placeholder state of the text field `key`.
pub fn field_selector(key: &str) -> String {
    if key == "search" {
        return "#searchbox".to_owned();
    }
    format!("#fld-{}", field_id(key))
}

/// Element id fragment of a field key (`compositor.terminal` becomes `compositor-terminal`).
pub fn field_id(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Inline icon.
pub fn icon(name: &str, size: u32) -> String {
    icon_svg(name, size, "")
}

/// The whole body: sidebar plus main area plus the tooltip layer.
pub fn document(ctx: &Ctx<'_>) -> String {
    format!(
        "<div class='app{compact}' id=app>\
         <div class=side>\
         <div class='search fld{filled}' id=searchbox>{search}<span class=phd>{placeholder}</span>\
         <input id=search data-key=search data-submit=search value='{query}' autocomplete=off>\
         <span id=searchhint class=kbd>/</span><span class=clear data-action=search-clear>{close}</span></div>\
         <div id=nav class=navlist>{nav}</div>\
         <div class=foot>{spark}<span>{footer}</span></div>\
         </div>\
         <div class=main id=main>{main}</div>\
         <div id=tip></div></div>",
        compact = if ctx.model.compact { " compact" } else { "" },
        filled = if ctx.model.query.is_empty() {
            ""
        } else {
            " filled"
        },
        placeholder = esc(&tr("ui.search.placeholder", "Search all settings")),
        footer = esc(&tr(
            "ui.sidebar.footer",
            "Every option of Desilium lives here. No config files, no tweak tools."
        )),
        query = esc(&ctx.model.query),
        close = icon("close", 12),
        search = icon("search", 16),
        nav = nav_html(ctx),
        spark = icon("sparkles", 16),
        main = main_html(ctx),
    )
}

/// Sidebar entries.
pub fn nav_html(ctx: &Ctx<'_>) -> String {
    let mut out = String::new();
    for entry in NAV {
        match entry {
            NavEntry::Separator => out.push_str("<div class=nav-sep></div>"),
            NavEntry::Planned {
                slug,
                label,
                icon: name,
            } => {
                let _ = write!(
                    out,
                    "<div class='nav dis' data-tooltip='{}'><span class=t>{}</span>{}</div>",
                    esc(&tr("ui.nav.coming_soon", PLANNED_TOOLTIP)),
                    icon(name, 16),
                    esc(&tr(&keys::page(slug), label))
                );
            }
            NavEntry::Page(page) => {
                let on = !ctx.model.searching() && ctx.model.page == page;
                let _ = write!(
                    out,
                    "<div class='nav{}' data-action=nav data-arg={}><span class=t>{}</span>{}</div>",
                    if on { " on" } else { "" },
                    page.slug(),
                    icon(page.icon(), 16),
                    esc(&page.title())
                );
            }
        }
    }
    out
}

/// Banner (if any) plus the page or the search results.
pub fn main_html(ctx: &Ctx<'_>) -> String {
    let body = if ctx.model.searching() {
        search_page::html(ctx)
    } else {
        page::html(ctx)
    };
    format!("{}{body}", notice_html(ctx.model.notice.as_ref()))
}

fn notice_html(notice: Option<&Notice>) -> String {
    let Some(notice) = notice else {
        return String::new();
    };
    let (class, name) = match notice.level {
        Level::Info => ("notice", "info"),
        Level::Error => ("notice err", "info"),
    };
    let action = match notice.action {
        Some(Recovery::Reload) => format!(
            "<span class='btn sm' data-action=notice-reload>{}</span>",
            esc(&tr("ui.notice.reload", "Reload from disk"))
        ),
        Some(Recovery::Retry) => format!(
            "<span class='btn sm' data-action=notice-retry>{}</span>",
            esc(&tr("ui.notice.retry", "Try again"))
        ),
        Some(Recovery::Dismiss) | None => String::new(),
    };
    format!(
        "<div class='{class}' id=notice>{}<span class=msg>{}</span>{action}\
         <span class='btn sm ghost' data-action=notice-dismiss>{}</span></div>",
        icon(name, 16),
        esc(&notice.text),
        esc(&tr("ui.notice.dismiss", "Dismiss"))
    )
}

/// The "last change" row: message plus Undo, or a quiet hint when there is none.
pub fn toast_html(toast: Option<&Toast>, idle: bool) -> String {
    let Some(toast) = toast else {
        if !idle {
            return String::new();
        }
        return format!(
            "<div class='undo idle'>{}<span class=msg>{}</span></div>",
            icon("check", 16),
            esc(&tr(
                "ui.toast.idle",
                "Changes are saved and applied right away."
            ))
        );
    };
    let undo = toast
        .undo
        .as_ref()
        .map(|_| {
            format!(
                "<span class='btn sm ghost' data-action=undo style='color:var(--text-on-surface-accent)'>{}</span>",
                esc(&tr("ui.toast.undo", "Undo"))
            )
        })
        .unwrap_or_default();
    format!(
        "<div class=undo>{}<span class=msg>{}</span>{undo}</div>",
        icon("check", 16),
        esc(&toast.message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_markup_and_quotes() {
        assert_eq!(
            esc("<a href=\"x\">'&'"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;"
        );
    }

    #[test]
    fn file_urls_are_percent_encoded() {
        assert_eq!(
            file_url(Path::new("/a b/c#d.png")),
            "file:///a%20b/c%23d.png"
        );
        assert_eq!(file_url(Path::new("/x/y-z_1.png")), "file:///x/y-z_1.png");
        assert_eq!(file_url(Path::new("/ü")), "file:///%C3%BC");
    }
}
