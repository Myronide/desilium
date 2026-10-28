//! Search results replacing the page content.

use super::{Ctx, esc, icon, toast_html};
use crate::{
    i18n::{tr, tr_args},
    search::{Hit, search},
};

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let hits = search(&ctx.model.query);
    let query = ctx.model.query.trim();
    let count = match hits.len() {
        0 => tr("ui.search.no_matches", "No matches"),
        1 => tr("ui.count.one_setting", "1 setting"),
        many => tr_args(
            "ui.count.settings",
            "{count} settings",
            &[("count", &many.to_string())],
        ),
    };
    let body = if hits.is_empty() {
        empty(query)
    } else {
        let rows: String = hits.iter().map(result).collect();
        format!("<div class=results>{rows}</div>")
    };
    let summary = tr_args(
        "ui.search.summary",
        "{count} for \"{query}\". Choose one to jump to it.",
        &[("count", &count), ("query", query)],
    );
    format!(
        "<div class=ph><div class='col grow'><h1>{}</h1>\
         <span class=sub>{}</span></div></div>\
         <div id=toast>{}</div>{body}",
        esc(&tr("ui.search.title", "Search")),
        esc(&summary),
        toast_html(ctx.model.toast.as_ref(), false)
    )
}

fn result(hit: &Hit) -> String {
    let def = hit.def;
    format!(
        "<div class=res data-action=goto data-arg={id}><span class=t>{}</span>\
         <div class=body><b>{}</b><span>{}</span></div>\
         <div class=where><span class=key>{id}</span><span class=chip>{}</span></div></div>",
        icon(def.page.icon(), 16),
        esc(&def.title_text()),
        esc(&def.description_text()),
        esc(&def.page.title()),
        id = def.id
    )
}

fn empty(query: &str) -> String {
    format!(
        "<div class=empty>{}<b>{}</b><span>{}</span></div>",
        icon("search", 28),
        esc(&tr_args(
            "ui.search.empty_title",
            "Nothing matches \"{query}\"",
            &[("query", query)]
        )),
        esc(&tr(
            "ui.search.empty_hint",
            "Try fewer or different words, for example \"accent\", \"wallpaper\" or \"shortcut\"."
        ))
    )
}
