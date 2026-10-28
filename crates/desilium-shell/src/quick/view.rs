//! The quick settings panel: round icon buttons, two sliders and a grid of pill tiles.
//!
//! Labels are one word at most; state is shown by colour (accent = on) and by the icon.

use crate::{
    services::controls::{LinkKind, State, WifiNetwork},
    view::esc,
};
use desilium_config_types::{PanelMode, ShellSettings};
use desilium_ui::icon_svg;

/// Segments of a slider; each one sets `100 / LEVEL_STEPS` percent more.
const LEVEL_STEPS: u8 = 20;
const ICON_PX: u32 = 20;
const SMALL_ICON_PX: u32 = 14;

/// What the panel shows besides the controls' own state.
#[derive(Debug, Clone, Copy)]
pub struct Look<'a> {
    /// The dark theme is active.
    pub dark: bool,
    /// Battery charge such as `87%`; empty on machines without a battery.
    pub battery: &'a str,
    /// The Wi-Fi network list is unfolded.
    pub networks_open: bool,
}

fn icon(name: &str) -> String {
    icon_svg(name, ICON_PX, "s20")
}

fn round_button(action: &str, arg: &str, icon_name: &str, title: &str) -> String {
    format!(
        "<button class=round data-action='{action}' data-arg='{}' data-tooltip='{}'>{}</button>",
        esc(arg),
        esc(title),
        icon(icon_name)
    )
}

/// A pill that switches something on or off; the accent colour means on.
fn tile(icon_name: &str, label: &str, on: bool, action: &str, arg: &str) -> String {
    format!(
        "<button class='tile{}' data-action='{action}' data-arg='{}'>{}<span>{}</span></button>",
        if on { " on" } else { "" },
        esc(arg),
        icon(icon_name),
        esc(label)
    )
}

/// A pill whose service is missing: shown dimmed, does nothing.
fn unavailable(icon_name: &str, label: &str) -> String {
    format!(
        "<div class='tile unavailable'>{}<span>{}</span></div>",
        icon(icon_name),
        esc(label)
    )
}

/// A pill with a main part that toggles and an arrow part that opens more.
fn split_tile(
    icon_name: &str,
    label: &str,
    on: bool,
    (action, arg): (&str, &str),
    more: &str,
) -> String {
    format!(
        "<div class='tile split{}'><button class=main data-action='{action}' data-arg='{}'>{}<span>{}</span></button><button class=more data-action='{more}'>{}</button></div>",
        if on { " on" } else { "" },
        esc(arg),
        icon(icon_name),
        esc(label),
        icon("chevron-right")
    )
}

/// An icon, a track with a knob and invisible segments on top that take the clicks.
fn slider(lead: &str, action: &str, value: u8) -> String {
    let step = 100 / LEVEL_STEPS;
    let value = value.min(100);
    let segments: String = (1..=LEVEL_STEPS)
        .map(|index| {
            format!(
                "<button class=seg data-action='{action}' data-arg='{}'></button>",
                index * step
            )
        })
        .collect();
    format!(
        "<div class=slider>{lead}<div class=track><div class=fill style='flex:{value} 1 0'></div><div class=knob></div><div class=rest style='flex:{} 1 0'></div><div class=hits>{segments}</div></div></div>",
        100 - value
    )
}

fn volume_row(state: &State) -> String {
    let Some(value) = state.volume else {
        return String::new();
    };
    let lead = round_button(
        "mute",
        &(!state.muted).to_string(),
        if state.muted { "volume-mute" } else { "volume" },
        if state.muted { "Unmute" } else { "Mute" },
    );
    slider(&lead, "volume", if state.muted { 0 } else { value })
}

fn brightness_row(state: &State) -> String {
    state.brightness.map_or_else(String::new, |value| {
        slider(
            &format!("<span class=lead>{}</span>", icon("sun")),
            "brightness",
            value,
        )
    })
}

fn wifi_tile(state: &State) -> String {
    let Some(on) = state.wifi else {
        return unavailable("wifi", "Wi-Fi");
    };
    let label = match state.links.as_deref().and_then(<[_]>::first) {
        Some(link) if link.kind == LinkKind::Wifi => link.name.as_str(),
        _ => "Wi-Fi",
    };
    split_tile("wifi", label, on, ("wifi", &(!on).to_string()), "networks")
}

fn bluetooth_tile(state: &State) -> String {
    state.bluetooth.map_or_else(
        || unavailable("bluetooth", "Bluetooth"),
        |on| {
            tile(
                "bluetooth",
                "Bluetooth",
                on,
                "bluetooth",
                &(!on).to_string(),
            )
        },
    )
}

/// Cycles saver, balanced, performance with each click.
fn profile_tile(state: &State) -> String {
    let Some(current) = state.profile.as_deref() else {
        return unavailable("bolt", "Power");
    };
    let (label, next) = match current {
        "power-saver" => ("Saver", "balanced"),
        "performance" => ("Performance", "power-saver"),
        _ => ("Balanced", "performance"),
    };
    tile("bolt", label, true, "profile", next)
}

/// `microphone` holds whether the microphone is muted; the pill is lit while it is live.
fn microphone_tile(state: &State) -> String {
    state.microphone.map_or_else(
        || unavailable("mic", "Mic"),
        |muted| tile("mic", "Mic", !muted, "microphone", &(!muted).to_string()),
    )
}

fn wifi_list(networks: &[WifiNetwork]) -> String {
    let rows: String = networks
        .iter()
        .map(|network| {
            format!(
                "<button class='ap{}' data-action=wifi-connect data-arg='{}'><span>{}</span><small>{}{}</small></button>",
                if network.active { " active" } else { "" },
                esc(&network.ssid),
                esc(&network.ssid),
                if network.secured {
                    icon_svg("lock", SMALL_ICON_PX, "s16")
                } else {
                    String::new()
                },
                network.signal
            )
        })
        .collect();
    format!("<div class=aps>{rows}</div>")
}

pub fn html(state: &State, settings: &ShellSettings, look: Look<'_>) -> String {
    let taskbar = settings.mode == PanelMode::Taskbar;
    let battery = if look.battery.is_empty() {
        String::new()
    } else {
        format!(
            "<span class=batt>{}<span>{}</span></span>",
            icon("battery"),
            esc(look.battery)
        )
    };
    let networks = if look.networks_open && !state.networks.is_empty() {
        wifi_list(&state.networks)
    } else {
        String::new()
    };
    let notice = if state.notice.is_empty() {
        String::new()
    } else {
        format!("<p class=notice>{}</p>", esc(&state.notice))
    };
    format!(
        "<div class=panel style='border-radius:{radius}px'><div class=top>{settings_button}<span class=grow></span>{battery}{close}</div>{volume}{brightness}<div class=tiles>{wifi}{bluetooth}{profile}{mic}{theme}{mode}</div>{networks}{notice}</div>",
        radius = settings.popup_radius,
        settings_button = round_button("settings", "", "settings", "Settings"),
        close = round_button("close", "", "close", "Close"),
        volume = volume_row(state),
        brightness = brightness_row(state),
        wifi = wifi_tile(state),
        bluetooth = bluetooth_tile(state),
        profile = profile_tile(state),
        mic = microphone_tile(state),
        theme = tile("moon", "Dark", look.dark, "theme", ""),
        mode = tile(
            "layers",
            "Taskbar",
            taskbar,
            "mode",
            if taskbar { "dock" } else { "taskbar" }
        ),
    )
}

fn button(action: &str, arg: &str, label: &str) -> String {
    format!(
        "<button data-action='{action}' data-arg='{}'>{}</button>",
        esc(arg),
        esc(label)
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
        html.push_str(&button("dock-desktop", &menu.key, "Add to desktop"));
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
