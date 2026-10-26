//! Row layouts and the HTML of every control kind.

use std::fmt::Write;

use super::{Ctx, esc, field_id, file_url, icon, page::modified_link};
use crate::{
    palette::{ACCENT_PRESETS, preset_for},
    registry::{Choice, Control, SettingDef, Slider, Stepper, Value, format_number},
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
        esc(def.title),
        esc(def.description),
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
        esc(def.title),
        esc(&caption),
        modified_link(ctx, def, true)
    )
}

/// "Off", "1.5x", "32 px": the value of one stop.
fn stop_caption(slider: &Slider, stop: f64) -> String {
    if stop <= 0.0 {
        return "Off".to_owned();
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
        return format!("{label} \u{b7} Default");
    }
    label
}

pub(super) fn control(ctx: &Ctx<'_>, def: &'static SettingDef) -> String {
    let value = (def.get)(ctx.config);
    match def.control {
        Control::Segmented(choices) => segmented(def, choices, &value),
        Control::Select(choices) => select(ctx, def, choices, &value),
        Control::Switch => switch(def, &value),
        Control::Slider(slider) => slider_html(def, &slider, &value),
        Control::Stepper(stepper) => stepper_html(def, &stepper, &value),
        Control::CursorTheme => cursor_theme(ctx, def, &value),
        Control::Text { placeholder } => text_input(def, placeholder, &value),
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
            esc(choice.label)
        );
    }
    out.push_str("</div>");
    out
}

fn select(ctx: &Ctx<'_>, def: &SettingDef, choices: &[Choice], value: &Value) -> String {
    let current = choices
        .iter()
        .find(|choice| matches!(value, Value::Text(text) if text == choice.value))
        .map_or("", |choice| choice.label);
    let open = ctx.model.open_select == Some(def.id);
    let menu = if open {
        options_menu(def, choices, value)
    } else {
        String::new()
    };
    format!(
        "<div class=btn data-action=select-toggle data-arg={}>{}{}</div>{menu}",
        def.id,
        esc(current),
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
            esc(choice.label)
        );
    }
    out.push_str("</div>");
    out
}

fn cursor_theme(ctx: &Ctx<'_>, def: &SettingDef, value: &Value) -> String {
    let mut choices = vec![("", "System default".to_owned())];
    choices.extend(
        ctx.model
            .cursor_themes
            .iter()
            .map(|name| (name.as_str(), name.clone())),
    );
    let current = match value {
        Value::Text(name) => name.clone(),
        _ => "System default".to_owned(),
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

fn stepper_html(def: &SettingDef, stepper: &Stepper, value: &Value) -> String {
    let Value::Number(number) = value else {
        return String::new();
    };
    let at_min = *number <= stepper.min as f64;
    let at_max = *number >= stepper.max as f64;
    let button = |sign: &str, delta: &str, disabled: bool| {
        if disabled {
            format!("<span class='btn dis'>{sign}</span>")
        } else {
            format!(
                "<span class=btn data-action=step data-arg='{}={delta}'>{sign}</span>",
                def.id
            )
        }
    };
    format!(
        "<div class=stepper>{}<span class=val>{}</span>{}</div>",
        button("\u{2212}", "-1", at_min),
        format_number(*number),
        button("+", "1", at_max)
    )
}

fn text_input(def: &SettingDef, placeholder: &str, value: &Value) -> String {
    let current = match value {
        Value::Text(text) => text.as_str(),
        _ => "",
    };
    format!(
        "<div class='input fld{}' id='fld-{fid}' style='width:260px'><span class=phd>{}</span>\
         <input id='in-{fid}' data-key='{id}' data-submit='text:{id}' value='{}' autocomplete=off>\
         <span class=kbd>Enter</span></div>",
        if current.is_empty() { "" } else { " filled" },
        esc(placeholder),
        esc(current),
        id = def.id,
        fid = field_id(def.id)
    )
}

fn slider_html(def: &SettingDef, slider: &Slider, value: &Value) -> String {
    let Value::Number(number) = value else {
        return String::new();
    };
    let current = slider.index_of(*number);
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
        let _ = write!(
            ticks,
            "<span class='{}' style='left:{:.3}%'>{}</span>",
            if index == current { "on" } else { "" },
            slider.fraction(index) * 100.0,
            esc(text)
        );
    }
    format!(
        "<div class=slide><div class=trk><div class=bar></div><div class=fill style='width:{percent:.3}%'></div>\
         <div class=thumb style='left:{percent:.3}%'></div>\
         <div class=hits style='left:-{half:.3}%;width:{total:.3}%'>{hits}</div></div>\
         <div class=tickrow>{ticks}</div></div>",
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
            esc(preset.name),
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
        Value::Text(hex) if preset_for(hex).is_none() => format!("Custom colour {}", esc(hex)),
        Value::Text(hex) => format!(
            "{} (preset)",
            esc(preset_for(hex).map_or(hex.as_str(), |p| p.name))
        ),
        _ => format!("Using the theme's own accent {theme_accent}"),
    };
    let clear = if matches!(value, Value::Text(_)) {
        "<span class='btn sm ghost' data-action=clear data-arg=appearance.accent>Use theme accent</span>"
    } else {
        ""
    };
    let error = error
        .map(|text| format!("<span class=err>{}</span>", esc(&text)))
        .unwrap_or_default();
    format!(
        "<div class=hexrow><div class='input fld{}' id=fld-accent-hex><span class=phd>#7B61FF</span><input id=hex data-key=accent-hex data-submit=accent-apply value='{}' autocomplete=off></div>\
         <span class='btn sm' data-action=accent-apply>{}Apply</span>{clear}<span class=hint>{}</span>{error}</div>",
        if draft.is_empty() { "" } else { " filled" },
        esc(&draft),
        icon("palette", 16),
        shown
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
        "<div class='wp{}' data-action=clear data-arg={}><div class='th theme'>{}</div>Theme default</div>",
        if on { " on" } else { "" },
        def.id,
        if on { check_badge() } else { String::new() }
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
        "No wallpapers found. Put images into ~/.local/share/desilium/wallpapers and they show up here."
    } else {
        "To add more, put images into ~/.local/share/desilium/wallpapers."
    };
    format!("<div class=note>{}</div>", esc(text))
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
    format!(
        "<div class=note>Current wallpaper: {} (not in the installed collection)</div>",
        esc(&display_name(std::path::Path::new(path)))
    )
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
