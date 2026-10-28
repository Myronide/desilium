//! Keyboard page body: the shortcut list with in-place re-binding.

use super::{Ctx, esc};
use crate::{
    i18n::{tr, tr_args},
    model::Rebind,
    shortcuts::{action_label, chord_parts, chord_text, default_for},
};
use desilium_config_types::Shortcut;

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let shortcuts = &ctx.config.compositor.shortcuts;
    let rows: String = shortcuts
        .iter()
        .enumerate()
        .map(|(index, shortcut)| row(ctx, index, shortcut))
        .collect();
    let help = tr(
        "ui.keyboard.help",
        "Choose Change, then press the new combination. Shortcuts need Alt, Ctrl or Super \
         together with a key. Esc cancels.",
    );
    format!("<div class='note first'>{}</div>{rows}", esc(&help))
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
    let name = esc(&action_label(shortcut.action));
    if let Some(rebind) = rebind {
        return capturing_row(ctx, &name, rebind);
    }
    let modified = is_modified(shortcut);
    let note = if modified {
        esc(&tr("ui.keyboard.changed", "Changed from the default"))
    } else {
        String::new()
    };
    let reset = if modified {
        format!(
            "<span class='btn sm ghost' data-action=shortcut-reset data-arg={index}>{}</span>",
            esc(&tr("ui.keyboard.reset", "Reset"))
        )
    } else {
        String::new()
    };
    let hl = ctx.model.highlighted(ctx.now) == Some("compositor.shortcuts");
    format!(
        "<div class='sc{}' id='sc-{index}'><div class=name>{name}<span>{note}</span></div>\
         <div class=keys>{}</div>{reset}\
         <span class='btn sm' data-action=rebind data-arg={index}>{}</span></div>",
        if hl && index == 0 { " hl" } else { "" },
        keycaps(shortcut),
        esc(&tr("ui.keyboard.change", "Change"))
    )
}

fn capturing_row(ctx: &Ctx<'_>, name: &str, rebind: &Rebind) -> String {
    let index = rebind.index;
    let cancel = esc(&tr("ui.keyboard.cancel", "Cancel"));
    if let Some(pending) = &rebind.pending {
        let other = ctx.config.compositor.shortcuts.get(pending.conflict);
        let other_name = other.map_or_else(
            || tr("ui.keyboard.another_action", "another action"),
            |s| action_label(s.action),
        );
        let chord = chord_text(
            desilium_config_types::Modifiers {
                alt: pending.modifiers[0],
                ctrl: pending.modifiers[1],
                shift: pending.modifiers[2],
                logo: pending.modifiers[3],
            },
            &pending.key,
        );
        let question = tr_args(
            "ui.keyboard.conflict",
            "{chord} is already used by \"{action}\". Swap the two shortcuts?",
            &[("chord", &chord), ("action", &other_name)],
        );
        return format!(
            "<div class='sc cap' id='sc-{index}'><div class=name>{name}<span>{}</span></div>\
             <span class='btn sm primary' data-action=rebind-swap>{}</span>\
             <span class='btn sm' data-action=rebind-cancel>{cancel}</span></div>",
            esc(&question),
            esc(&tr("ui.keyboard.swap", "Swap"))
        );
    }
    let message = rebind
        .message
        .clone()
        .unwrap_or_else(|| tr("ui.keyboard.press_now", "Press the new shortcut now..."));
    format!(
        "<div class='sc cap' id='sc-{index}'><div class=name>{name}<span>{}</span></div>\
         <span class='btn sm' data-action=rebind-cancel>{cancel}</span></div>",
        esc(&message)
    )
}
