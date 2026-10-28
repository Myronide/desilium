//! Row layouts and the HTML of every control kind.

use std::fmt::Write;

use super::{Ctx, esc, field_id, file_url, icon, page::modified_link};
use crate::{
    i18n::{tr, tr_args},
    palette::{ACCENT_PRESETS, preset_for},
    registry::{Choice, Control, OFF_LABEL, SettingDef, Slider, Stepper, Value, format_number},
    wallpapers::display_name,
};

/// Label column: title, description and the per-setting reset link.
fn label(ctx: &Ctx<'_>, def: &SettingDef) -> String {
    let single = crate::registry::for_page(def.page)
        .filter(|other| other.section == def.section)
        .count()
        == 1;
    format!(
        "<div class=lab><b>{}</b><span>{}</span>{}</div>",
        esc(&def.title_text()),
        esc(&def.description_text()),
        modified_link(ctx, def, single)
    )
}

fn row_class(ctx: &Ctx<'_>, def: &SettingDef, extra: &str) -> String {
    let hl = ctx.model.highlighted(ctx.now) == Some(def.id);
    format!("srow{extra}{}", if hl { " hl" } else { "" })
}

/// Label on the left, control on the right.
pub(super) fn plain_row(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    format!(
        "<div class='{}' id='row-{}'>{}<div class=ctl>{}</div></div>",
        row_class(ctx, def, ""),
        def.id,
        label(ctx, def),
        control(ctx, def)
    )
}

/// Label above-left, wide control (slider) filling the rest.
pub(super) fn stacked_row(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    format!(
        "<div class='{}' id='row-{}'>{}<div class=ctl>{}</div></div>",
        row_class(ctx, def, " stack"),
        def.id,
        slider_label(ctx, def),
        control(ctx, def)
    )
}

fn slider_label(ctx: &Ctx<'_>, def: &SettingDef) -> String {
    let Control::Slider(slider) = def.control else {
        return label(ctx, def);
    };
    let caption = slider_caption(&slider, &(def.get)(ctx.config), &def.default_value());
    format!(
        "<div class=lab><b>{}</b><span>{}</span>{}</div>",
        esc(&def.title_text()),
        esc(&caption),
        modified_link(ctx, def, true)
    )
}

/// "Off", "1.5x", "32 px": the value of one stop.
fn stop_caption(slider: &Slider, stop: f64) -> String {
    if stop <= 0.0 {
        return off_text();
    }
    let gap = if slider.unit == "px" { " " } else { "" };
    format!("{}{gap}{}", format_number(stop), slider.unit)
}

/// "1.0x . Default", "Off", "32 px".
fn slider_caption(slider: &Slider, value: &Value, default: &Value) -> String {
    let Value::Number(number) = value else {
        return String::new();
    };
    let index = slider.index_of(*number);
    let label = slider
        .stops
        .get(index)
        .map(|stop| stop_caption(slider, *stop))
        .unwrap_or_default();
    if value == default {
        return tr_args(
            "ui.slider.is_default",
            "{value} \u{b7} Default",
            &[("value", &label)],
        );
    }
    label
}

/// The lowest slider stop.
fn off_text() -> String {
    tr("ui.common.off", OFF_LABEL)
}

pub(super) fn control(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    let value = (def.get)(ctx.config);
    match def.control {
        Control::Segmented(choices) => segmented(def, choices, &value),
        Control::Select(choices) => select(ctx, def, choices, &value),
        Control::Switch => switch(def, &value),
        Control::Slider(slider) => slider_html(ctx, def, &slider, &value),
        Control::Stepper(stepper) => stepper_html(ctx, def, &stepper, &value),
        Control::CursorTheme => cursor_theme(ctx, def, &value),
        Control::Text { .. } => text_input(def, &value),
        Control::Accent | Control::Wallpaper | Control::Shortcuts | Control::About => String::new(),
    }
}

fn arg(def: &SettingDef, value: &str) -> String {
    format!("{}={}", def.id, esc(value))
}

fn segmented(def: &SettingDef, choices: &[Choice], value: &Value) -> String {
    let mut out = String::from("<div class=seg>");
    for choice in choices {
        let on = matches!(value, Value::Text(text) if text == choice.value);
        let glyph = choice.icon.map(|name| icon(name, 16)).unwrap_or_default();
        let _ = write!(
            out,
            "<span class='{}' data-action=set data-arg='{}'>{glyph}{}</span>",
            if on { "on" } else { "" },
            arg(def, choice.value),
            esc(&choice.label_text(def.id))
        );
    }
    out.push_str("</div>");
    out
}

fn select(ctx: &Ctx<'_>, def: &SettingDef, choices: &[Choice], value: &Value) -> String {
    let current = choices
        .iter()
        .find(|choice| matches!(value, Value::Text(text) if text == choice.value))
        .map_or_else(String::new, |choice| choice.label_text(def.id));
    let open = ctx.model.open_select == Some(def.id);
    let menu = if open {
        options_menu(def, choices, value)
    } else {
        String::new()
    };
    format!(
        "<div class=btn data-action=select-toggle data-arg={}>{}{}</div>{menu}",
        def.id,
        esc(&current),
        icon("chevron-down", 16)
    )
}

fn options_menu(def: &SettingDef, choices: &[Choice], value: &Value) -> String {
    let mut out = String::from("<div class=menu>");
    for choice in choices {
        let on = matches!(value, Value::Text(text) if text == choice.value);
        let mark = if on { icon("check", 14) } else { String::new() };
        let _ = write!(
            out,
            "<div class='opt{}' data-action=set data-arg='{}'>{}<span class=grow>{}</span></div>",
            if on { " on" } else { "" },
            arg(def, choice.value),
            mark,
            esc(&choice.label_text(def.id))
        );
    }
    out.push_str("</div>");
    out
}

fn cursor_theme(ctx: &Ctx<'_>, def: &SettingDef, value: &Value) -> String {
    let system_default = tr("ui.cursor.system_default", "System default");
    let mut choices = vec![("", system_default.clone())];
    choices.extend(
        ctx.model
            .cursor_themes
            .iter()
            .map(|name| (name.as_str(), name.clone())),
    );
    let current = match value {
        Value::Text(name) => name.clone(),
        _ => system_default,
    };
    let open = ctx.model.open_select == Some(def.id);
    let menu = if open {
        cursor_menu(def, &choices, value)
    } else {
        String::new()
    };
    format!(
        "<div class=btn data-action=select-toggle data-arg={}>{}{}</div>{menu}",
        def.id,
        esc(&current),
        icon("chevron-down", 16)
    )
}

fn cursor_menu(def: &SettingDef, choices: &[(&str, String)], value: &Value) -> String {
    let mut out = String::from("<div class=menu>");
    for (id, label) in choices {
        let on = match value {
            Value::Text(name) => name == id,
            _ => id.is_empty(),
        };
        let action = if id.is_empty() {
            format!("data-action=clear data-arg={}", def.id)
        } else {
            format!("data-action=set data-arg='{}'", arg(def, id))
        };
        let mark = if on { icon("check", 14) } else { String::new() };
        let _ = write!(
            out,
            "<div class='opt{}' {action}>{mark}<span class=grow>{}</span></div>",
            if on { " on" } else { "" },
            esc(label)
        );
    }
    out.push_str("</div>");
    out
}

fn switch(def: &SettingDef, value: &Value) -> String {
    let on = matches!(value, Value::Bool(true));
    format!(
        "<span class='switch{}' data-action=set data-arg='{}'></span>",
        if on { " on" } else { "" },
        arg(def, if on { "false" } else { "true" })
    )
}

/// Text of a range's value: the number and its unit.
fn range_text(stepper: &Stepper, number: f64) -> String {
    let gap = if stepper.unit.is_empty() || stepper.unit == "%" {
        ""
    } else {
        " "
    };
    format!("{}{gap}{}", format_number(number), stepper.unit)
}

/// A number edited with a slider: drag the thumb, or click the track to jump. The value is
/// shown next to it. While dragging, the preview value is shown and nothing is stored yet.
fn stepper_html(ctx: &Ctx<'_>, def: &SettingDef, stepper: &Stepper, value: &Value) -> String {
    let Value::Number(stored) = value else {
        return String::new();
    };
    let number = match ctx.model.range_preview {
        Some((id, preview)) if id == def.id => preview,
        _ => *stored,
    };
    let percent = stepper.position(number) * 100.0;
    let mut hits = String::new();
    for step in stepper.min..=stepper.max {
        let _ = write!(
            hits,
            "<i data-action=set data-arg='{}'></i>",
            arg(def, &step.to_string())
        );
    }
    let fid = field_id(def.id);
    format!(
        "<div class=range><div class=trk id='trk-{fid}' data-drag=range data-arg='{id}'>\
         <div class=bar></div><div class=fill id='fill-{fid}' style='width:{percent:.3}%'></div>\
         <div class=thumb id='thumb-{fid}' style='left:{percent:.3}%'></div>\
         <div class=rhits>{hits}</div></div>\
         <span class=val id='val-{fid}'>{text}</span></div>",
        id = def.id,
        text = esc(&range_text(stepper, number))
    )
}

/// Document updates that move a range's thumb and value while it is dragged.
pub fn range_updates(def: &SettingDef, number: f64) -> Option<desilium_ui::Updates> {
    let fid = field_id(def.id);
    let updates = desilium_ui::Updates::new();
    let (percent, text) = match def.control {
        Control::Stepper(stepper) => (stepper.position(number), Some(range_text(&stepper, number))),
        Control::Slider(slider) => (slider.fraction(slider.index_of(number)), None),
        _ => return None,
    };
    let percent = percent * 100.0;
    let updates = updates
        .attr(
            &format!("#fill-{fid}"),
            "style",
            &format!("width:{percent:.3}%"),
        )
        .attr(
            &format!("#thumb-{fid}"),
            "style",
            &format!("left:{percent:.3}%"),
        );
    Some(match text {
        Some(text) => updates.text(&format!("#val-{fid}"), &text),
        None => updates,
    })
}

fn text_input(def: &SettingDef, value: &Value) -> String {
    let current = match value {
        Value::Text(text) => text.as_str(),
        _ => "",
    };
    format!(
        "<div class='input fld{}' id='fld-{fid}' style='width:260px'><span class=phd>{}</span>\
         <input id='in-{fid}' data-key='{id}' data-submit='text:{id}' value='{}' autocomplete=off>\
         <span class=kbd>{}</span></div>",
        if current.is_empty() { "" } else { " filled" },
        esc(&def.placeholder_text().unwrap_or_default()),
        esc(current),
        esc(&crate::shortcuts::key_label("Return")),
        id = def.id,
        fid = field_id(def.id)
    )
}

fn slider_html(ctx: &Ctx<'_>, def: &SettingDef, slider: &Slider, value: &Value) -> String {
    let Value::Number(stored) = value else {
        return String::new();
    };
    let number = match ctx.model.range_preview {
        Some((id, preview)) if id == def.id => preview,
        _ => *stored,
    };
    let current = slider.index_of(number);
    let count = slider.stops.len();
    let percent = slider.fraction(current) * 100.0;
    let step = 100.0 / count.saturating_sub(1).max(1) as f64;
    let mut hits = String::new();
    for stop in slider.stops {
        let _ = write!(
            hits,
            "<i data-action=set data-arg='{}' data-tooltip='{}'></i>",
            arg(def, &stop.to_string()),
            esc(&stop_caption(slider, *stop))
        );
    }
    let mut ticks = String::new();
    for (index, text) in slider.labels.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let shown = if *text == OFF_LABEL {
            off_text()
        } else {
            (*text).to_owned()
        };
        let _ = write!(
            ticks,
            "<span class='{}' style='left:{:.3}%'>{}</span>",
            if index == current { "on" } else { "" },
            slider.fraction(index) * 100.0,
            esc(&shown)
        );
    }
    format!(
        "<div class=slide><div class=trk id='trk-{fid}' data-drag=range data-arg='{id}'><div class=bar></div>\
         <div class=fill id='fill-{fid}' style='width:{percent:.3}%'></div>\
         <div class=thumb id='thumb-{fid}' style='left:{percent:.3}%'></div>\
         <div class=hits style='left:-{half:.3}%;width:{total:.3}%'>{hits}</div></div>\
         <div class=tickrow>{ticks}</div></div>",
        fid = field_id(def.id),
        id = def.id,
        half = step / 2.0,
        total = 100.0 + step
    )
}

/// Accent card body: swatches, then the custom colour line.
pub(super) fn accent(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    let value = (def.get)(ctx.config);
    let theme_accent = ctx.theme.palette.accent.to_hex().to_ascii_uppercase();
    let active = match &value {
        Value::Text(hex) => hex.to_ascii_uppercase(),
        _ => theme_accent.clone(),
    };
    let mut swatches = String::from("<div class=swatches>");
    for preset in ACCENT_PRESETS {
        let on = preset.hex.eq_ignore_ascii_case(&active);
        let check = if on {
            format!(
                "<span style='color:#fff;display:grid'>{}</span>",
                icon("check", 18)
            )
        } else {
            String::new()
        };
        let _ = write!(
            swatches,
            "<span class='sw{}' style='background:{hex};color:{hex}' data-action=set data-arg='{}' data-tooltip='{}'>{check}</span>",
            if on { " on" } else { "" },
            arg(def, preset.hex),
            esc(&preset.label()),
            hex = preset.hex,
        );
    }
    swatches.push_str("</div>");
    let highlighted = ctx.model.highlighted(ctx.now) == Some(def.id);
    format!(
        "<div id='row-{}'{}>{swatches}{}</div>",
        def.id,
        if highlighted { " class=hl" } else { "" },
        custom_accent(ctx, &value, &theme_accent)
    )
}

fn custom_accent(ctx: &Ctx<'_>, value: &Value, theme_accent: &str) -> String {
    let (draft, error) = match &ctx.model.accent_editor {
        Some(editor) => (editor.text.clone(), editor.error.clone()),
        None => match value {
            Value::Text(hex) => (hex.clone(), None),
            _ => (String::new(), None),
        },
    };
    let shown = match value {
        Value::Text(hex) => match preset_for(hex) {
            None => tr_args("ui.accent.custom", "Custom colour {hex}", &[("hex", hex)]),
            Some(preset) => tr_args(
                "ui.accent.preset",
                "{name} (preset)",
                &[("name", &preset.label())],
            ),
        },
        _ => tr_args(
            "ui.accent.theme_own",
            "Using the theme's own accent {hex}",
            &[("hex", theme_accent)],
        ),
    };
    let clear = if matches!(value, Value::Text(_)) {
        format!(
            "<span class='btn sm ghost' data-action=clear data-arg=appearance.accent>{}</span>",
            esc(&tr("ui.accent.use_theme", "Use theme accent"))
        )
    } else {
        String::new()
    };
    let error = error
        .map(|text| format!("<span class=err>{}</span>", esc(&text)))
        .unwrap_or_default();
    format!(
        "<div class=hexrow><div class='input fld{}' id=fld-accent-hex><span class=phd>#7B61FF</span><input id=hex data-key=accent-hex data-submit=accent-apply value='{}' autocomplete=off></div>\
         <span class='btn sm' data-action=accent-apply>{}{}</span>{clear}<span class=hint>{}</span>{error}</div>",
        if draft.is_empty() { "" } else { " filled" },
        esc(&draft),
        icon("palette", 16),
        esc(&tr("ui.accent.apply", "Apply")),
        esc(&shown)
    )
}

/// Wallpaper card body: thumbnail grid.
pub(super) fn wallpaper(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    let value = (def.get)(ctx.config);
    let current = match &value {
        Value::Text(path) => Some(path.as_str()),
        _ => None,
    };
    let mut grid = String::from("<div class=walls>");
    grid.push_str(&theme_tile(def, current.is_none()));
    for entry in &ctx.model.wallpapers {
        let path = entry.path.to_string_lossy();
        let on = current == Some(path.as_ref());
        grid.push_str(&tile(ctx, def, entry, &path, on));
    }
    grid.push_str("</div>");
    format!(
        "<div id='row-{}'>{grid}{}{}</div>",
        def.id,
        add_hint(ctx.model.wallpapers.is_empty()),
        missing_note(current, ctx)
    )
}

fn check_badge() -> String {
    format!("<span class=ck>{}</span>", icon("check", 12))
}

fn theme_tile(def: &SettingDef, on: bool) -> String {
    format!(
        "<div class='wp{}' data-action=clear data-arg={}><div class='th theme'>{}</div>{}</div>",
        if on { " on" } else { "" },
        def.id,
        if on { check_badge() } else { String::new() },
        esc(&tr("ui.wallpaper.theme_default", "Theme default"))
    )
}

fn tile(
    ctx: &Ctx<'_>,
    def: &SettingDef,
    entry: &crate::wallpapers::WallpaperEntry,
    path: &str,
    on: bool,
) -> String {
    let thumb = ctx
        .model
        .thumbs
        .get(&entry.path)
        .map(|png| format!("<img src='{}' width=102 height=64>", esc(&file_url(png))))
        .unwrap_or_default();
    format!(
        "<div class='wp{}' data-action=set data-arg='{}'><div class=th>{thumb}{}</div>{}</div>",
        if on { " on" } else { "" },
        arg(def, path),
        if on { check_badge() } else { String::new() },
        esc(&entry.name)
    )
}

/// Where to put more wallpapers (the picker has no file dialog yet).
fn add_hint(none_found: bool) -> String {
    let text = if none_found {
        tr(
            "ui.wallpaper.none_found",
            "No wallpapers found. Put images into ~/.local/share/desilium/wallpapers and they show up here.",
        )
    } else {
        tr(
            "ui.wallpaper.add_more",
            "To add more, put images into ~/.local/share/desilium/wallpapers.",
        )
    };
    format!("<div class=note>{}</div>", esc(&text))
}

/// A wallpaper path in the config that no longer exists in the installed list.
fn missing_note(current: Option<&str>, ctx: &Ctx<'_>) -> String {
    let Some(path) = current else {
        return String::new();
    };
    let known = ctx
        .model
        .wallpapers
        .iter()
        .any(|entry| entry.path.to_string_lossy() == path);
    if known {
        return String::new();
    }
    let note = tr_args(
        "ui.wallpaper.missing",
        "Current wallpaper: {name} (not in the installed collection)",
        &[("name", &display_name(std::path::Path::new(path)))],
    );
    format!("<div class=note>{}</div>", esc(&note))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_caption_marks_default_and_off() {
        let slider = Slider {
            stops: &[0.0, 1.0, 2.0],
            labels: &["Off", "1x", "2x"],
            unit: "x",
        };
        let default = Value::Number(1.0);
        assert_eq!(
            slider_caption(&slider, &Value::Number(1.0), &default),
            "1.0x \u{b7} Default"
        );
        assert_eq!(
            slider_caption(&slider, &Value::Number(0.0), &default),
            "Off"
        );
        assert_eq!(slider_caption(&slider, &Value::Number(2.0), &default), "2x");
        let px = Slider {
            stops: &[16.0, 24.0],
            labels: &["16", "24"],
            unit: "px",
        };
        assert_eq!(
            slider_caption(&px, &Value::Number(16.0), &Value::Number(24.0)),
            "16 px"
        );
    }
}
