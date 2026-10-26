//! Keyboard page body: the shortcut list with in-place re-binding.

use super::{Ctx, esc};
use crate::{
    model::Rebind,
    shortcuts::{action_label, chord_parts, chord_text, default_for},
};
use desilium_config_types::Shortcut;

const HELP: &str = "Choose Change, then press the new combination. Shortcuts need Alt, Ctrl or Super \
     together with a key. Esc cancels.";

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let shortcuts = &ctx.config.compositor.shortcuts;
    let rows: String = shortcuts
        .iter()
        .enumerate()
        .map(|(index, shortcut)| row(ctx, index, shortcut))
        .collect();
    format!("<div class=note style='padding-top:12px'>{HELP}</div>{rows}")
}

fn keycaps(shortcut: &Shortcut) -> String {
    chord_parts(shortcut.modifiers, &shortcut.key)
        .iter()
        .map(|part| format!("<span class=kbd>{}</span>", esc(part)))
        .collect::<Vec<_>>()
        .join("<span class=plus>+</span>")
}

fn is_modified(shortcut: &Shortcut) -> bool {
    default_for(shortcut.action).is_none_or(|default| {
        default.modifiers != shortcut.modifiers || !default.key.eq_ignore_ascii_case(&shortcut.key)
    })
}

fn row(ctx: &Ctx<'_>, index: usize, shortcut: &Shortcut) -> String {
    let rebind = ctx.model.rebind.as_ref().filter(|r| r.index == index);
    let name = esc(action_label(shortcut.action));
    if let Some(rebind) = rebind {
        return capturing_row(ctx, &name, rebind);
    }
    let modified = is_modified(shortcut);
    let note = if modified {
        "Changed from the default"
    } else {
        ""
    };
    let reset = if modified {
        format!(
            "<span class='btn sm ghost' data-action=shortcut-reset data-arg={index}>Reset</span>"
        )
    } else {
        String::new()
    };
    let hl = ctx.model.highlighted(ctx.now) == Some("compositor.shortcuts");
    format!(
        "<div class='sc{}' id='sc-{index}'><div class=name>{name}<span>{note}</span></div>\
         <div class=keys>{}</div>{reset}\
         <span class='btn sm' data-action=rebind data-arg={index}>Change</span></div>",
        if hl && index == 0 { " hl" } else { "" },
        keycaps(shortcut)
    )
}

fn capturing_row(ctx: &Ctx<'_>, name: &str, rebind: &Rebind) -> String {
    let index = rebind.index;
    if let Some(pending) = &rebind.pending {
        let other = ctx.config.compositor.shortcuts.get(pending.conflict);
        let other_name = other.map_or("another action", |s| action_label(s.action));
        let chord = chord_text(
            desilium_config_types::Modifiers {
                alt: pending.modifiers[0],
                ctrl: pending.modifiers[1],
                shift: pending.modifiers[2],
                logo: pending.modifiers[3],
            },
            &pending.key,
        );
        return format!(
            "<div class='sc cap' id='sc-{index}'><div class=name>{name}<span>{} is already used by \"{}\". \
             Swap the two shortcuts?</span></div>\
             <span class='btn sm primary' data-action=rebind-swap>Swap</span>\
             <span class='btn sm' data-action=rebind-cancel>Cancel</span></div>",
            esc(&chord),
            esc(other_name)
        );
    }
    let message = rebind
        .message
        .as_deref()
        .unwrap_or("Press the new shortcut now...");
    format!(
        "<div class='sc cap' id='sc-{index}'><div class=name>{name}<span>{}</span></div>\
         <span class='btn sm' data-action=rebind-cancel>Cancel</span></div>",
        esc(message)
    )
}
