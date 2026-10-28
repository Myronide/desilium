//! About page body.

use super::{Ctx, esc, icon};
use crate::i18n::tr;

pub(super) fn html(ctx: &Ctx<'_>) -> String {
    let info = &ctx.model.info;
    let dirs = info
        .wallpaper_dirs
        .iter()
        .map(|dir| esc(dir))
        .collect::<Vec<_>>()
        .join("<br>");
    format!(
        "<div class=hero><span class='app-icon lg a-settings'>{}</span>\
         <div class=col><h2>Desilium</h2><span>{}</span></div></div>\
         <div class=kvrow><span class=k>{}</span><span class=v>{}</span></div>\
         <div class=kvrow><span class=k>{}</span><span class=v>{}</span></div>\
         <div class=kvrow><span class=k>{}</span><span class=v>{dirs}</span></div>\
         <div class=kvrow><span class=k>{}</span><span class=v>{}</span></div>",
        icon("settings", 32),
        esc(&tr(
            "ui.about.tagline",
            "A calm, fast, keyboard-first desktop."
        )),
        esc(&tr("ui.about.version", "Version")),
        esc(&info.version),
        esc(&tr("ui.about.settings_file", "Settings file")),
        esc(&info.config_file),
        esc(&tr("ui.about.wallpaper_folders", "Wallpaper folders")),
        esc(&tr("ui.about.backups_kept", "Backups kept")),
        esc(&tr(
            "ui.about.backups_text",
            "The last 10 versions of the settings file are kept, so any change can be undone."
        )),
    )
}
