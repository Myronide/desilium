//! The "Get new themes" page: a card for the KDE Store (tabs, search, results, paging) and a
//! card listing what is installed.

use std::fmt::Write;

use desilium_config_types::KDE_THEME;

use super::{Ctx, esc, icon};
use crate::{
    i18n::{tr, tr_args},
    shortcuts::key_label,
    store::{Entry, Kind, Load, StoreState},
};

/// Longest installed list drawn.
const MAX_INSTALLED_ROWS: usize = 200;

fn tab_label(kind: Kind) -> String {
    match kind {
        Kind::Icons => tr("ui.store.tab_icons", "Icons"),
        Kind::Cursors => tr("ui.store.tab_cursors", "Cursors"),
        Kind::Colours => tr("ui.store.tab_colours", "Colours"),
    }
}

/// Whether the theme `name` of `kind` is the one in use.
fn in_use(ctx: &Ctx<'_>, kind: Kind, name: &str) -> bool {
    let appearance = &ctx.config.appearance;
    match kind {
        Kind::Icons => appearance.icon_theme.as_deref() == Some(name),
        Kind::Cursors => appearance.cursor_theme.as_deref() == Some(name),
        Kind::Colours => {
            appearance.theme == KDE_THEME && appearance.kde_scheme.as_deref() == Some(name)
        }
    }
}

fn tabs(state: &StoreState) -> String {
    let mut out = String::from("<div class=seg>");
    for kind in Kind::ALL {
        let _ = write!(
            out,
            "<span class='{}' data-action=store-tab data-arg={}>{}</span>",
            if kind == state.kind { "on" } else { "" },
            kind.slug(),
            esc(&tab_label(kind))
        );
    }
    out.push_str("</div>");
    out
}

fn search_field(state: &StoreState) -> String {
    format!(
        "<div class='input fld{filled}' id='fld-store-search' style='width:300px'>\
         <span class=phd>{placeholder}</span>\
         <input id='in-store-search' data-key='store-search' data-submit='store-search' value='{draft}' autocomplete=off>\
         <span class=kbd>{enter}</span></div>",
        filled = if state.draft.is_empty() {
            ""
        } else {
            " filled"
        },
        placeholder = esc(&tr("ui.store.search", "Search the KDE Store")),
        draft = esc(&state.draft),
        enter = esc(&key_label("Return")),
    )
}

fn meta(entry: &Entry) -> String {
    let downloads = tr_args(
        "ui.store.downloads",
        "{count} downloads",
        &[("count", &entry.downloads.to_string())],
    );
    if entry.author.is_empty() {
        downloads
    } else {
        format!("{} \u{b7} {downloads}", entry.author)
    }
}

fn button(label: &str, action: &str, arg: &str) -> String {
    format!(
        "<span class='btn sm' data-action={action} data-arg='{}'>{}</span>",
        esc(arg),
        esc(label)
    )
}

fn disabled_button(label: &str) -> String {
    format!("<span class='btn sm dis'>{}</span>", esc(label))
}

/// The buttons of a result row: install, a pending state, or "use" for what it installed.
fn entry_actions(state: &StoreState, entry: &Entry, index: usize) -> String {
    if state.installing.as_deref() == Some(entry.id.as_str()) {
        return disabled_button(&tr("ui.store.installing", "Installing\u{2026}"));
    }
    let installed: Vec<&String> = state
        .installed_from(&entry.id)
        .iter()
        .filter(|name| state.installed.contains(name))
        .collect();
    if installed.is_empty() {
        let busy = state.installing.is_some();
        let label = tr("ui.store.install", "Install");
        return if busy {
            disabled_button(&label)
        } else {
            button(&label, "store-install", &index.to_string())
        };
    }
    installed
        .into_iter()
        .map(|name| {
            let label = if state.installed_from(&entry.id).len() > 1 {
                tr_args("ui.store.use_named", "Use {name}", &[("name", name)])
            } else {
                tr("ui.store.use", "Use")
            };
            button(&label, "store-use", name)
        })
        .collect()
}

fn results(state: &StoreState) -> String {
    let message = |text: String| format!("<div class=note>{}</div>", esc(&text));
    match &state.load {
        Load::Idle => message(tr(
            "ui.store.idle",
            "Type a search and press Enter, or open another tab, to list themes.",
        )),
        Load::Loading => message(tr("ui.store.loading", "Loading\u{2026}")),
        Load::Failed(text) => format!(
            "<div class='srow'><div class=lab><b>{}</b><span>{}</span></div><div class=ctl>{}</div></div>",
            esc(&tr("ui.store.failed", "The store could not be reached")),
            esc(text),
            button(&tr("ui.store.retry", "Try again"), "store-reload", "")
        ),
        Load::Ready if state.listing.entries.is_empty() => {
            message(tr("ui.store.nothing", "Nothing found."))
        }
        Load::Ready => {
            let mut out = String::new();
            for (index, entry) in state.listing.entries.iter().enumerate() {
                let _ = write!(
                    out,
                    "<div class=srow><div class=lab><b>{}</b><span>{}</span></div><div class=ctl>{}</div></div>",
                    esc(&entry.name),
                    esc(&meta(entry)),
                    entry_actions(state, entry, index)
                );
            }
            out
        }
    }
}

fn paging(state: &StoreState) -> String {
    if state.load != Load::Ready || (!state.has_next() && !state.has_previous()) {
        return String::new();
    }
    let previous = tr("ui.store.previous", "Previous");
    let next = tr("ui.store.next", "Next");
    let page = tr_args(
        "ui.store.page",
        "Page {page} of {pages}",
        &[
            ("page", &(state.page + 1).to_string()),
            ("pages", &state.pages().to_string()),
        ],
    );
    format!(
        "<div class=srow><div class=lab><span>{}</span></div><div class=ctl>{}{}</div></div>",
        esc(&page),
        if state.has_previous() {
            button(&previous, "store-page", "prev")
        } else {
            disabled_button(&previous)
        },
        if state.has_next() {
            button(&next, "store-page", "next")
        } else {
            disabled_button(&next)
        },
    )
}

fn store_card(state: &StoreState) -> String {
    format!(
        "<div class=card id=card-store>\
         <div class=sh><h3>{title}</h3><span class=grow></span>\
         <span class='btn sm' data-action=store-site>{globe}{site}</span></div>\
         <div class=srow><div class=lab><span>{hint}</span></div><div class=ctl>{tabs}</div></div>\
         <div class=srow><div class=ctl>{search}</div></div>\
         {results}{paging}</div>",
        title = esc(&tr("ui.store.title", "KDE Store")),
        globe = icon("globe", 16),
        site = esc(&tr("ui.store.open_site", "Open store website")),
        hint = esc(&tr(
            "ui.store.hint",
            "Themes are downloaded from store.kde.org and installed for your user only."
        )),
        tabs = tabs(state),
        search = search_field(state),
        results = results(state),
        paging = paging(state),
    )
}

fn installed_card(ctx: &Ctx<'_>, state: &StoreState) -> String {
    let mut rows = String::new();
    for name in state.installed.iter().take(MAX_INSTALLED_ROWS) {
        let action = if in_use(ctx, state.kind, name) {
            format!(
                "<span class='chip accent'>{}{}</span>",
                icon("check", 14),
                esc(&tr("ui.store.in_use", "In use"))
            )
        } else {
            button(&tr("ui.store.use", "Use"), "store-use", name)
        };
        let _ = write!(
            rows,
            "<div class=srow><div class=lab><b>{}</b></div><div class=ctl>{action}</div></div>",
            esc(name)
        );
    }
    if rows.is_empty() {
        rows = format!(
            "<div class=note>{}</div>",
            esc(&tr("ui.store.none_installed", "Nothing installed yet."))
        );
    }
    format!(
        "<div class=card id=card-installed><div class=sh><h3>{}</h3></div>{rows}</div>",
        esc(&tr(
            "ui.store.installed_title",
            "Installed on this computer"
        ))
    )
}

/// The page body (the header is drawn by the page module).
pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let state = &ctx.model.store;
    format!(
        "<div id=toast>{}</div><div class=wide>{}{}</div>",
        super::toast_html(ctx.model.toast.as_ref(), false),
        store_card(state),
        installed_card(ctx, state),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::{app::tests::app_in, store::Listing};

    fn entry(id: &str, name: &str) -> Entry {
        Entry {
            id: id.to_owned(),
            name: name.to_owned(),
            author: "ann".to_owned(),
            downloads: 12,
            score: 80,
            detail_page: None,
            files: Vec::new(),
        }
    }

    fn page_html(app: &crate::app::App) -> String {
        html(&app.ctx(Instant::now()))
    }

    #[test]
    fn results_are_escaped_and_offer_install_by_index() {
        let (_dir, mut app) = app_in();
        let state = &mut app.model_mut().store;
        state.load = Load::Ready;
        state.listing = Listing {
            total: 2,
            entries: vec![entry("1", "A <b>bold</b> pack"), entry("2", "Plain")],
        };
        let html = page_html(&app);
        assert!(html.contains("A &lt;b&gt;bold&lt;/b&gt; pack"));
        assert!(!html.contains("<b>bold</b>"));
        assert!(html.contains("data-action=store-install data-arg='0'"));
        assert!(html.contains("data-action=store-install data-arg='1'"));
        assert!(html.contains("ann \u{b7} 12 downloads"));
        assert!(html.contains("data-action=store-tab data-arg=cursors"));
    }

    #[test]
    fn an_installed_entry_offers_use_and_a_busy_one_cannot_start_another() {
        let (_dir, mut app) = app_in();
        let state = &mut app.model_mut().store;
        state.load = Load::Ready;
        state.listing = Listing {
            total: 2,
            entries: vec![entry("1", "Done"), entry("2", "Other")],
        };
        state.installed = vec!["Done".to_owned()];
        state.by_id.insert("1".to_owned(), vec!["Done".to_owned()]);
        let html = page_html(&app);
        assert!(html.contains("data-action=store-use data-arg='Done'"));
        app.model_mut().store.installing = Some("2".to_owned());
        let html = page_html(&app);
        assert!(html.contains("Installing"));
        assert!(
            !html.contains("data-action=store-install"),
            "one download at a time"
        );
    }

    #[test]
    fn loading_failure_and_empty_states_say_so() {
        let (_dir, mut app) = app_in();
        app.model_mut().store.load = Load::Loading;
        assert!(page_html(&app).contains("Loading"));
        app.model_mut().store.load = Load::Failed("No network".to_owned());
        let html = page_html(&app);
        assert!(html.contains("No network") && html.contains("data-action=store-reload"));
        app.model_mut().store.load = Load::Ready;
        assert!(page_html(&app).contains("Nothing found."));
    }

    #[test]
    fn the_installed_card_marks_the_theme_in_use() {
        let (_dir, mut app) = app_in();
        app.model_mut().store.installed = vec!["Papirus".to_owned(), "Breeze".to_owned()];
        let now = Instant::now();
        app.handle_event(
            &desilium_ui::UiEvent::Action(desilium_ui::UiAction {
                action: "set".into(),
                arg: Some("appearance.icon_theme=Papirus".into()),
                element_id: None,
                rect: Default::default(),
            }),
            now,
        );
        let html = page_html(&app);
        assert!(html.contains("In use"));
        assert!(html.contains("data-action=store-use data-arg='Breeze'"));
        assert!(!html.contains("data-action=store-use data-arg='Papirus'"));
    }
}
