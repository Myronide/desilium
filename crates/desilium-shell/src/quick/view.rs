use crate::{
    services::controls::{LinkKind, State, WifiNetwork},
    view::esc,
};
use desilium_config_types::{PanelMode, ShellSettings};
/// Segments of a level bar; each one sets `100 / LEVEL_STEPS` percent more.
const LEVEL_STEPS: u8 = 20;
fn button(action: &str, arg: &str, label: &str) -> String {
    format!(
        "<button data-action='{action}' data-arg='{}'>{}</button>",
        esc(arg),
        esc(label)
    )
}
fn toggle(action: &str, title: &str, value: Option<bool>, on_text: &str, off_text: &str) -> String {
    let Some(on) = value else {
        return format!(
            "<div class='tile unavailable'><strong>{title}</strong><small>Unavailable</small></div>"
        );
    };
    format!(
        "<button class='tile {}' data-action={action} data-arg={}><strong>{title}</strong><small>{}</small></button>",
        if on { "on" } else { "" },
        !on,
        if on { on_text } else { off_text }
    )
}
/// A row with a title, a clickable level bar and the value.
fn level(title: &str, action: &str, value: u8, extra: &str) -> String {
    let step = 100 / LEVEL_STEPS;
    let segments: String = (1..=LEVEL_STEPS)
        .map(|index| {
            let target = index * step;
            format!(
                "<button class='seg{}' data-action={action} data-arg={target}></button>",
                if value + step / 2 >= target { " on" } else { "" }
            )
        })
        .collect();
    format!(
        "<div class=slider><div class=slider-head><strong>{title}</strong><span>{value}%</span>{extra}</div><div class=level>{segments}</div></div>"
    )
}
fn network_summary(state: &State) -> String {
    let (title, detail, connected) = match &state.links {
        None => ("Network", "NetworkManager unavailable".to_owned(), false),
        Some(links) => match links.first() {
            Some(link) => (
                match link.kind {
                    LinkKind::Ethernet => "Ethernet",
                    LinkKind::Wifi => "Wi-Fi",
                },
                format!("Connected to {}", esc(&link.name)),
                true,
            ),
            None => ("Network", "Not connected".to_owned(), false),
        },
    };
    format!(
        "<div class='net{}'><div class=dot></div><div><strong>{title}</strong><small>{detail}</small></div></div>",
        if connected { " up" } else { "" }
    )
}
fn wifi_list(networks: &[WifiNetwork]) -> String {
    if networks.is_empty() {
        return String::new();
    }
    let rows: String = networks
        .iter()
        .map(|network| {
            format!(
                "<button class='ap{}' data-action=wifi-connect data-arg='{}'><span>{}</span><small>{}{}%</small></button>",
                if network.active { " active" } else { "" },
                esc(&network.ssid),
                esc(&network.ssid),
                if network.secured { "🔒 " } else { "" },
                network.signal
            )
        })
        .collect();
    format!("<div class=aps><h3>Wi-Fi networks</h3>{rows}</div>")
}
pub fn html(state: &State, settings: &ShellSettings, battery: &str) -> String {
    let volume = state.volume.map_or_else(
        || "<p class=hint>Audio control unavailable</p>".into(),
        |value| {
            level(
                "Volume",
                "volume",
                value,
                &button(
                    "mute",
                    &(!state.muted).to_string(),
                    if state.muted { "Unmute" } else { "Mute" },
                ),
            )
        },
    );
    let brightness = state.brightness.map_or_else(
        || "<p class=hint>Display brightness control unavailable</p>".into(),
        |value| level("Brightness", "brightness", value, ""),
    );
    let profile = state.profile.as_ref().map_or_else(String::new, |current| {
        let choice = |id: &str, label: &str| {
            format!(
                "<button class='{}' data-action=profile data-arg={id}>{label}</button>",
                if current == id { "chosen" } else { "" }
            )
        };
        format!(
            "<div class=power><h3>Power profile</h3><div class=profile>{}{}{}</div></div>",
            choice("power-saver", "Save"),
            choice("balanced", "Balanced"),
            choice("performance", "Performance")
        )
    });
    let mode = if settings.mode == PanelMode::Dock {
        "taskbar"
    } else {
        "dock"
    };
    let notice = if state.notice.is_empty() {
        String::new()
    } else {
        format!("<p class=notice>{}</p>", esc(&state.notice))
    };
    format!(
        "<div class=panel><div class=heading><div><h2>Quick settings</h2><small>{}</small></div><div class=heading-actions>{}<button data-action=close>×</button></div></div>{}<div class=tiles>{}{}{}{}</div><div class=sliders>{volume}{brightness}</div>{}{profile}<div class=panel-footer>{}{}{}</div>{notice}</div>",
        esc(battery),
        button("settings", "", "⚙"),
        network_summary(state),
        toggle("wifi", "Wi-Fi", state.wifi, "On", "Off"),
        toggle("bluetooth", "Bluetooth", state.bluetooth, "On", "Off"),
        toggle("microphone", "Microphone", state.microphone, "Muted", "Live"),
        "<button class=tile data-action=theme><strong>Appearance</strong><small>Light / Dark</small></button>",
        wifi_list(&state.networks),
        button(
            "mode",
            mode,
            if mode == "dock" {
                "Use Dock"
            } else {
                "Use Taskbar"
            }
        ),
        button("panel-settings", "", "Panels…"),
        button("refresh", "", "Refresh"),
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
