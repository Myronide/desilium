//! A settings page: header, cards generated from the registry, right-hand column.

use std::fmt::Write;

use super::{Ctx, about, controls, esc, icon, keyboard, preview};
use crate::{
    i18n::{tr, tr_args},
    registry::{Control, Page, Section, SettingDef, sections},
};

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let page = ctx.model.page;
    if page == Page::Themes {
        return format!("{}{}", header(ctx), super::themes::html(ctx));
    }
    let cards: String = sections(page)
        .iter()
        .map(|section| card(ctx, section))
        .collect();
    let body = if has_side_column(page) {
        format!(
            "<div class=cols><div class=c1>{cards}</div><div class=c2>{}</div></div>",
            preview::column(ctx)
        )
    } else {
        format!(
            "<div id=toast>{}</div><div class=wide>{cards}</div>",
            super::toast_html(ctx.model.toast.as_ref(), false)
        )
    };
    format!("{}{body}", header(ctx))
}

fn has_side_column(page: Page) -> bool {
    matches!(page, Page::Appearance | Page::Desktop)
}

fn header(ctx: &Ctx<'_>) -> String {
    let page = ctx.model.page;
    let pill = if page.has_preview() {
        tr("ui.header.live_preview", "Live preview")
    } else {
        tr("ui.header.applied_instantly", "Applied instantly")
    };
    let reset = if page_is_default(ctx, page) || page == Page::About {
        String::new()
    } else {
        format!(
            "<span class='btn sm' data-action=reset-page data-arg={}>{}{}</span>",
            page.slug(),
            icon("restore", 16),
            esc(&tr("ui.header.reset_page", "Reset page"))
        )
    };
    let live = if matches!(page, Page::About | Page::Themes) {
        String::new()
    } else {
        format!("<span class=live><i></i>{}</span>", esc(&pill))
    };
    format!(
        "<div class=ph><div class='col grow'><h1>{}</h1><span class=sub>{}</span></div>{live}{reset}</div>",
        esc(&page.title()),
        esc(&page.subtitle())
    )
}

fn page_is_default(ctx: &Ctx<'_>, page: Page) -> bool {
    crate::registry::for_page(page).all(|def| def.is_default(ctx.config))
}

fn card(ctx: &Ctx<'_>, section: &Section) -> String {
    let Some(first) = section.settings.first() else {
        return String::new();
    };
    let highlighted = section
        .settings
        .iter()
        .any(|def| ctx.model.highlighted(ctx.now) == Some(def.id) && def.fills_card());
    let rows: String = section.settings.iter().map(|def| row(ctx, def)).collect();
    format!(
        "<div class='card{}' id='card-{}'>{}{rows}</div>",
        if highlighted { " hl" } else { "" },
        first.id,
        card_header(ctx, section)
    )
}

fn card_header(ctx: &Ctx<'_>, section: &Section) -> String {
    let defs = &section.settings;
    let key = match defs.as_slice() {
        [only] => only.id.to_owned(),
        many => tr_args(
            "ui.count.settings",
            "{count} settings",
            &[("count", &many.len().to_string())],
        ),
    };
    let informational = defs.iter().all(|def| def.is_informational());
    let reset = if informational {
        String::new()
    } else {
        reset_badge(ctx, section)
    };
    let key = if informational {
        String::new()
    } else {
        format!("<span class=key>{}</span>", esc(&key))
    };
    format!(
        "<div class=sh><h3>{}</h3>{key}<span class=grow></span>{reset}</div>",
        esc(&section.title_text())
    )
}

/// "Default" when nothing in the card is changed, otherwise a Reset button for the card.
fn reset_badge(ctx: &Ctx<'_>, section: &Section) -> String {
    let changed = section
        .settings
        .iter()
        .any(|def| !def.is_default(ctx.config));
    if !changed {
        return format!(
            "<span class=reset>{}{}</span>",
            icon("restore", 14),
            esc(&tr("ui.card.default", "Default"))
        );
    }
    format!(
        "<span class='reset on' data-action=reset-section data-arg='{}/{}' data-tooltip='{}'>{}{}</span>",
        ctx.model.page.slug(),
        esc(section.title),
        esc(&tr("ui.card.reset_tooltip", "Back to the default")),
        icon("restore", 14),
        esc(&tr("ui.card.reset", "Reset"))
    )
}

fn row(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    match def.control {
        Control::Accent => controls::accent(ctx, def),
        Control::Wallpaper => controls::wallpaper(ctx, def),
        Control::Shortcuts => keyboard::html(ctx),
        Control::About => about::html(ctx),
        Control::Slider(_) => controls::stacked_row(ctx, def),
        _ => controls::plain_row(ctx, def),
    }
}

/// Shared by rows: the "modified" reset link under a label.
pub(super) fn modified_link(ctx: &Ctx<'_>, def: &SettingDef, single_in_card: bool) -> String {
    if def.is_default(ctx.config) || single_in_card {
        return String::new();
    }
    let mut out = String::new();
    let _ = write!(
        out,
        "<span class=mod data-action=reset data-arg={}>{}</span>",
        def.id,
        esc(&tr("ui.row.modified_reset", "Modified. Reset"))
    );
    out
}
