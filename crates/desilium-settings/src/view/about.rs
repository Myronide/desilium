//! About page body.

use super::{Ctx, esc, icon};

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
         <div class=col><h2>Desilium</h2><span>A calm, fast, keyboard-first desktop.</span></div></div>\
         <div class=kvrow><span class=k>Version</span><span class=v>{}</span></div>\
         <div class=kvrow><span class=k>Settings file</span><span class=v>{}</span></div>\
         <div class=kvrow><span class=k>Wallpaper folders</span><span class=v>{dirs}</span></div>\
         <div class=kvrow><span class=k>Backups kept</span><span class=v>The last 10 versions of the settings file are kept, so any change can be undone.</span></div>",
        icon("settings", 32),
        esc(&info.version),
        esc(&info.config_file),
    )
}
