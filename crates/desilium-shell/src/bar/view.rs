//! HTML of the top bar, built from a plain [`BarView`].

use desilium_ui::icon_svg;

use crate::{
    model::{
        battery::{Battery, BatteryStatus},
        clock::ClockText,
        windows::Workspaces,
    },
    view::{
        esc,
        icon::{IconSpec, icon_html},
    },
};

/// Height of the bar in logical pixels (also its exclusive zone).
pub const BAR_HEIGHT: u32 = 32;
const ICON_PX: u32 = 16;

/// The focused application as the bar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveApp {
    /// Application name.
    pub name: String,
    /// The application's own icon.
    pub icon: IconSpec,
    /// Window title (empty when there is none).
    pub title: String,
}

/// Everything the bar displays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarView {
    /// Workspace pills.
    pub workspaces: Workspaces,
    /// Focused window, or the desktop.
    pub app: ActiveApp,
    /// Date and time text.
    pub clock: ClockText,
    /// Battery, hidden when absent.
    pub battery: Option<Battery>,
    /// Keyboard layout label.
    pub layout: String,
}

fn icon(name: &str) -> String {
    icon_svg(name, ICON_PX, "s16")
}

/// Workspace pills; the active one is the long accent pill.
pub fn workspaces_html(workspaces: Workspaces) -> String {
    let pills: String = (0..workspaces.count)
        .map(|index| {
            let on = if index == workspaces.active { " on" } else { "" };
            format!(
                "<span class=\"ws-pill{on}\" data-action=workspace data-arg={index} data-tooltip=\"Workspace {}\"><i></i></span>",
                index + 1
            )
        })
        .collect();
    format!("<span class=\"ws-dots\">{pills}</span>")
}

/// Left cluster: workspaces, application, window title.
pub fn left_html(view: &BarView) -> String {
    let title = if view.app.title.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"tb-btn dim title static\" data-tooltip=\"{0}\">{0}</div>",
            esc(&view.app.title)
        )
    };
    format!(
        "{}<div class=\"tb-btn bar-app static\">{}<span class=t-semi>{}</span></div>{title}",
        workspaces_html(view.workspaces),
        icon_html(&view.app.icon, "", ICON_PX),
        esc(&view.app.name),
    )
}

fn battery_html(battery: Battery) -> String {
    let bolt = if battery.status == BatteryStatus::Charging {
        icon("bolt")
    } else {
        String::new()
    };
    format!(
        "<span class=bat data-tooltip=\"Battery\">{bolt}{}<span class=pct>{}%</span></span>",
        icon("battery"),
        battery.percent
    )
}

/// Right cluster: layout, status group (quick-settings button) and power.
///
/// The wifi and volume icons are static placeholders until a network and an audio
/// service feed them (planned: NetworkManager and PipeWire/PulseAudio over D-Bus).
pub fn right_html(view: &BarView) -> String {
    let battery = view.battery.map(battery_html).unwrap_or_default();
    format!(
        "<div class=\"tb-btn dim bar-layout\" data-action=layout data-tooltip=\"Keyboard layout (click to switch)\">{layout}</div>\
         <div class=tb-group data-action=quick-settings data-tooltip=\"Quick settings\">{wifi}{volume}{battery}{chevron}</div>\
         <div class=\"tb-btn dim bar-power\" data-action=power data-tooltip=\"Power\">{power}</div>",
        layout = esc(&view.layout),
        wifi = icon("wifi"),
        volume = icon("volume"),
        chevron = icon("chevron-down"),
        power = icon("power"),
    )
}

/// Centre: date and time.
pub fn clock_html(clock: &ClockText) -> String {
    format!(
        "<span class=date id=date>{}</span><span id=time>{}</span>",
        esc(&clock.date),
        esc(&clock.time)
    )
}

/// The whole bar document body.
pub fn bar_html(view: &BarView) -> String {
    format!(
        "<div class=bar><div class=left id=left>{}</div>\
         <div class=\"tb-btn tb-clock\" id=clock data-action=calendar data-tooltip=\"Calendar and notifications\">{}</div>\
         <div class=right id=right>{}</div></div>",
        left_html(view),
        clock_html(&view.clock),
        right_html(view)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> BarView {
        BarView {
            workspaces: Workspaces {
                count: 4,
                active: 1,
            },
            app: ActiveApp {
                name: "Code".into(),
                icon: IconSpec::Glyph("code"),
                title: "main.rs <desilium>".into(),
            },
            clock: ClockText {
                date: "Mon 26 Oct".into(),
                time: "14:32".into(),
            },
            battery: Some(Battery {
                percent: 82,
                status: BatteryStatus::Discharging,
            }),
            layout: "EN".into(),
        }
    }

    #[test]
    fn one_pill_per_workspace_and_exactly_one_active() {
        let html = workspaces_html(Workspaces {
            count: 4,
            active: 2,
        });
        assert_eq!(html.matches("ws-pill").count(), 4);
        assert_eq!(html.matches("ws-pill on").count(), 1);
        assert!(html.contains("data-action=workspace data-arg=2"));
    }

    #[test]
    fn titles_are_escaped() {
        let html = left_html(&view());
        assert!(html.contains("main.rs &lt;desilium&gt;"));
        assert!(!html.contains("<desilium>"));
    }

    #[test]
    fn missing_title_omits_the_title_element() {
        let mut v = view();
        v.app.title.clear();
        assert!(!left_html(&v).contains("title static"));
    }

    #[test]
    fn battery_is_hidden_without_one_and_shows_the_percentage() {
        let mut v = view();
        assert!(right_html(&v).contains("82%"));
        v.battery = None;
        assert!(!right_html(&v).contains("pct"));
    }

    #[test]
    fn charging_adds_a_bolt() {
        let mut v = view();
        let plain = right_html(&v);
        v.battery = Some(Battery {
            percent: 50,
            status: BatteryStatus::Charging,
        });
        assert!(right_html(&v).len() > plain.len());
    }

    #[test]
    fn the_status_group_is_the_quick_settings_button() {
        assert!(right_html(&view()).contains("data-action=quick-settings"));
        assert!(right_html(&view()).contains("data-action=power"));
    }
}
