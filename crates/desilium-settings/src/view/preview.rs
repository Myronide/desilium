//! Right-hand column: live preview, last change, "what this page controls".

use super::{Ctx, esc, file_url, icon, toast_html};
use crate::{
    model::Model,
    palette::preset_for,
    registry::{self, Page, Value},
};
use desilium_theme::Mode;

const PHILOSOPHY: &str = "These values are stored for you and can be exported or reset at any time. \
     <span class=t-2>You never need to edit a file.</span>";

pub(super) fn column(ctx: &Ctx<'_>) -> String {
    let toast = format!(
        "<div id=toast>{}</div>",
        toast_html(ctx.model.toast.as_ref(), true)
    );
    let first = if ctx.model.page.has_preview() {
        format!(
            "<div class='card pv pvw'>{}{toast}</div>",
            preview_body(ctx)
        )
    } else {
        toast
    };
    format!(
        "{first}<div class='card pv'><h3>What this page controls</h3>{}</div>\
         <div class=philo>{}<span>{PHILOSOPHY}</span></div>",
        controls_box(ctx),
        icon("info", 20)
    )
}

fn scheme_chip(ctx: &Ctx<'_>) -> String {
    let (glyph, mode) = match ctx.theme.mode {
        Mode::Dark => ("moon", "Dark"),
        Mode::Light => ("sun", "Light"),
    };
    let hex = ctx.theme.palette.accent.to_hex().to_ascii_uppercase();
    let accent = preset_for(&hex).map_or(hex.as_str(), |preset| preset.name);
    format!(
        "<span class='chip accent'>{}{mode} \u{b7} {}</span>",
        icon(glyph, 16),
        esc(accent)
    )
}

/// The wallpaper behind the mini desktop. An `<img>` rather than a CSS background: images
/// that arrive after the first frame (rendered in the background) load reliably as elements.
fn wallpaper_image(model: &Model) -> String {
    match &model.preview {
        Some(preview) => format!(
            "<img class=wall src='{}' width=100% height=100%>",
            esc(&file_url(&preview.png))
        ),
        None => String::new(),
    }
}

fn preview_body(ctx: &Ctx<'_>) -> String {
    let apps = ["files", "web", "code", "music", "settings"]
        .iter()
        .map(|app| format!("<i style='background:linear-gradient(150deg,var(--app-{app}))'></i>"))
        .collect::<String>();
    format!(
        "<div class=row><h3 class=grow>Preview</h3>{}</div>\
         <div class=mini>{}\
         <div class=mb><i style='left:8px;width:30px'></i><i style='left:50%;margin-left:-14px;width:28px'></i><i style='right:8px;width:40px'></i></div>\
         <div class=mw2><div class=mt></div></div>\
         <div class=mw><div class=mt></div><div class='ln a'></div><div class=ln style='width:140px'></div><div class=ln style='width:110px'></div><div class=ln style='width:150px'></div></div>\
         <div class=md>{apps}</div></div>",
        scheme_chip(ctx),
        wallpaper_image(ctx.model)
    )
}

/// The config keys of the page with their current values.
fn controls_box(ctx: &Ctx<'_>) -> String {
    let rows: String = registry::for_page(ctx.model.page)
        .filter(|def| !def.is_informational())
        .map(|def| {
            format!(
                "<div><span>{}</span><b>{}</b></div>",
                def.id,
                esc(&shown_value(ctx, def))
            )
        })
        .collect();
    format!("<div class=kv>{rows}</div>")
}

fn shown_value(ctx: &Ctx<'_>, def: &registry::SettingDef) -> String {
    let value = (def.get)(ctx.config);
    match (&value, def.page) {
        (Value::Text(path), Page::Appearance) if def.id == "appearance.wallpaper" => {
            std::path::Path::new(path)
                .file_stem()
                .map_or_else(|| path.clone(), |stem| stem.to_string_lossy().into_owned())
        }
        (Value::Unset, _) if def.id == "appearance.accent" => {
            ctx.theme.palette.accent.to_hex().to_ascii_uppercase()
        }
        (Value::Number(number), _) if def.id == "appearance.animation_speed" && *number <= 0.0 => {
            "off".to_owned()
        }
        _ => value.display(),
    }
}
