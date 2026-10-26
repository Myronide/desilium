//! Search results replacing the page content.

use super::{Ctx, esc, icon, toast_html};
use crate::search::{Hit, search};

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let hits = search(&ctx.model.query);
    let query = esc(ctx.model.query.trim());
    let count = match hits.len() {
        0 => "No matches".to_owned(),
        1 => "1 setting".to_owned(),
        many => format!("{many} settings"),
    };
    let body = if hits.is_empty() {
        empty(&query)
    } else {
        let rows: String = hits.iter().map(result).collect();
        format!("<div class=results>{rows}</div>")
    };
    format!(
        "<div class=ph><div class='col grow' style='gap:2px'><h1>Search</h1>\
         <span class=sub>{count} for \"{query}\". Choose one to jump to it.</span></div></div>\
         <div id=toast>{}</div>{body}",
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
        esc(def.title),
        esc(def.description),
        def.page.title(),
        id = def.id
    )
}

fn empty(query: &str) -> String {
    format!(
        "<div class=empty>{}<b>Nothing matches \"{query}\"</b>\
         <span>Try fewer or different words, for example \"accent\", \"wallpaper\" or \"shortcut\".</span></div>",
        icon("search", 28)
    )
}
