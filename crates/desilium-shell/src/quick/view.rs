use crate::{services::controls::State, view::esc};
use desilium_config_types::{PanelMode, ShellSettings};
fn button(action: &str, arg: &str, label: &str) -> String {
    format!(
        "<button data-action='{action}' data-arg='{}'>{}</button>",
        esc(arg),
        esc(label)
    )
}
fn toggle(action: &str, title: &str, value: Option<bool>) -> String {
    let Some(on) = value else {
        return format!(
            "<div class='tile unavailable'><strong>{title}</strong><small>Unavailable</small></div>"
        );
    };
    format!(
        "<button class='tile {}' data-action={action} data-arg={}><strong>{title}</strong><small>{}</small></button>",
        if on { "on" } else { "" },
        !on,
        if on { "On" } else { "Off" }
    )
}
pub fn html(state: &State, settings: &ShellSettings, battery: &str) -> String {
    let volume = state.volume.map_or_else(
        || "<p class=hint>Audio control unavailable</p>".into(),
        |value| {
            format!(
                "<div class=setting><strong>Volume</strong><span>{}%</span>{}{}{}</div>",
                value,
                button("volume", &value.saturating_sub(5).to_string(), "−"),
                button("volume", &(value + 5).min(100).to_string(), "+"),
                button(
                    "mute",
                    &(!state.muted).to_string(),
                    if state.muted { "Unmute" } else { "Mute" }
                )
            )
        },
    );
    let brightness = state.brightness.map_or_else(
        || "<p class=hint>Display brightness control unavailable</p>".into(),
        |value| {
            format!(
                "<div class=setting><strong>Brightness</strong><span>{value}%</span>{}{}</div>",
                button(
                    "brightness",
                    &value.saturating_sub(5).max(5).to_string(),
                    "−"
                ),
                button("brightness", &(value + 5).min(100).to_string(), "+")
            )
        },
    );
    let profile = state.profile.as_ref().map_or_else(String::new, |current| {
        format!(
            "<div class=power><strong>Power · {}</strong><div class=profile>{}{}{}</div></div>",
            esc(current),
            button("profile", "power-saver", "Save"),
            button("profile", "balanced", "Balanced"),
            button("profile", "performance", "Performance")
        )
    });
    let mode = if settings.mode == PanelMode::Dock {
        "taskbar"
    } else {
        "dock"
    };
    format!(
        "<div class=panel><div class=heading><div><h2>Quick settings</h2><small>{}</small></div><button data-action=close>×</button></div><div class=tiles>{}{}{}<button class=tile data-action=settings><strong>Settings</strong><small>All desktop controls</small></button></div>{volume}{brightness}{profile}<div class=setting><strong>Appearance</strong>{}</div><div class=setting><strong>Layout</strong>{}{}</div><p class=notice>{}</p><div class=panel-footer>{}{}</div></div>",
        esc(battery),
        toggle("wifi", "Wi-Fi", state.wifi),
        toggle("bluetooth", "Bluetooth", state.bluetooth),
        toggle("microphone", "Microphone muted", state.microphone),
        button("theme", "", "Light / Dark"),
        button(
            "mode",
            mode,
            if mode == "dock" {
                "Switch to Dock"
            } else {
                "Switch to Taskbar"
            }
        ),
        button("panel-settings", "", "Customize…"),
        esc(&state.notice),
        button("refresh", "", "Refresh status"),
        button("settings", "", "All settings")
    )
}

pub fn app_menu(menu: &super::AppMenu, notice: &str) -> String {
    let mut html = format!(
        "<div class=panel><div class=heading><h2>{}</h2><button data-action=close>×</button></div>",
        esc(&menu.name)
    );
    if menu.can_pin {
        html.push_str(&button(
            "dock-pin",
            &menu.key,
            if menu.pinned {
                "Unpin from panel"
            } else {
                "Pin to panel"
            },
        ));
        html.push_str(&button("dock-new", &menu.key, "New window"));
        if menu.pinned {
            html.push_str(&format!(
                "<div class=profile>{}{}</div>",
                button("dock-left", &menu.key, "Move left"),
                button("dock-right", &menu.key, "Move right")
            ));
        }
    }
    html.push_str("<h3>Open windows</h3>");
    for (id, title) in &menu.windows {
        html.push_str(&format!("<div class=setting><button data-action=window-activate data-arg={id}>{}</button><button data-action=window-close data-arg={id}>Close</button></div>",esc(title)));
    }
    if menu.windows.is_empty() {
        html.push_str("<p class=hint>No open windows.</p>");
    }
    html.push_str(&format!("<p class=notice>{}</p></div>", esc(notice)));
    html
}
