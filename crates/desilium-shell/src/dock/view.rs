//! HTML of the dock tile.

use desilium_ui::icon_svg;

use super::layout::DockLayout;
use crate::{
    model::dock::Dot,
    view::{
        esc,
        icon::{IconSpec, icon_html},
    },
};

/// Pixel size of an app icon inside its cell.
const ICON_PX: u32 = 46;

/// One app cell as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockCell {
    /// Stable key sent back with clicks.
    pub key: String,
    /// Tooltip text.
    pub name: String,
    /// Icon.
    pub icon: IconSpec,
    /// Dots under the icon.
    pub dots: Vec<Dot>,
}

/// Everything the dock draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockView {
    /// App cells in order.
    pub cells: Vec<DockCell>,
    /// Index of the first cell after the separator.
    pub separator: Option<usize>,
}

fn dots_html(dots: &[Dot]) -> String {
    if dots.is_empty() {
        return String::new();
    }
    let inner: String = dots
        .iter()
        .map(|dot| match dot {
            Dot::Focused => "<i class=on></i>",
            Dot::Open => "<i></i>",
        })
        .collect();
    format!("<span class=dots>{inner}</span>")
}

fn cell_html(index: usize, cell: &DockCell) -> String {
    format!(
        "<div class=dock-item id=dock-{index} data-action=app data-middle-action=app-new data-secondary-action=app-menu data-arg=\"{key}\" data-tooltip=\"{name}\">{icon}{dots}</div>",
        key = esc(&cell.key),
        name = esc(&cell.name),
        icon = icon_html(&cell.icon, "", ICON_PX),
        dots = dots_html(&cell.dots),
    )
}

/// The dock document body for `view` laid out as `layout`.
pub fn dock_html(view: &DockView, layout: &DockLayout) -> String {
    let mut inner = format!(
        "<div class=dock-item id=dock-launcher data-action=launcher data-tooltip=\"Applications\"><span class=dock-launcher>{}</span></div>",
        icon_svg("grid", 24, "")
    );
    for (index, cell) in view.cells.iter().enumerate() {
        if view.separator == Some(index) {
            inner.push_str("<div class=dock-sep></div>");
        }
        inner.push_str(&cell_html(index, cell));
    }
    format!(
        "<div class=dock id=dock style=\"width:{}px\">{inner}</div>",
        layout.width
    )
}

/// Corner radius of a taskbar (also the compositor's clip radius).
pub const TASKBAR_RADIUS: u32 = 18;

/// What the taskbar's right cluster shows besides the fixed tray icons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// `14:32`.
    pub time: String,
    /// `26 Oct`.
    pub date: String,
    /// Battery charge in percent, when there is a battery.
    pub battery: Option<u8>,
}

/// CSS that only a taskbar needs: a flat hover/focus highlight instead of the dock's lifting icons.
const TASKBAR_CSS: &str = ".dock-item:hover .app-icon,.dock-item:hover .app-img{transform:none;box-shadow:none}.dock-item{border-radius:10px}.dock-item:hover,.dock-item.focused{background:var(--surface-3)}.dock-tool{display:grid;place-items:center;color:var(--text-2)}.dock-item:hover .dock-tool{color:var(--text)}.dock-launcher{background:transparent;border:0}.task-system{display:flex;align-items:center;justify-content:flex-end;gap:2px}.task-system button{display:flex;align-items:center;gap:6px;background:transparent;border:0;border-radius:8px;padding:0 8px;height:100%;cursor:pointer;color:var(--text)}.task-system button:hover{background:var(--surface-3)}.task-system .tray{color:var(--text-2)}.task-system .clock{flex-direction:column;align-items:flex-end;justify-content:center;gap:1px;line-height:1.1}.task-system .clock b{font-size:13px;font-weight:600}.task-system .clock small{font-size:11px;color:var(--text-2)}.task-system .gear{padding:0 9px}.task-system .pct{font-size:11px}.dock .task-system button{background:transparent}.dock .task-system button:hover{background:var(--surface-3)}";

fn system_html(
    rect: desilium_ui::Rect,
    settings: &desilium_config_types::ShellSettings,
    status: &Status,
) -> String {
    let action = if settings.quick_settings {
        "quick-settings"
    } else {
        "none"
    };
    let battery = match (settings.show_battery, status.battery) {
        (true, Some(percent)) => format!(
            "{}<span class=pct>{percent}%</span>",
            icon_svg("battery", 18, "")
        ),
        _ => String::new(),
    };
    let clock = if settings.show_clock {
        format!(
            "<button class=clock data-action={action} data-tooltip='Calendar and notifications'><b>{}</b><small>{}</small></button>",
            esc(&status.time),
            esc(&status.date)
        )
    } else {
        String::new()
    };
    format!(
        "<div class=task-system style='{}'><button class=tray data-action={action} data-tooltip='Quick settings'>{}{}{battery}</button>{clock}<button class=gear data-action=settings data-tooltip='Settings'>{}</button></div>",
        rect_style(rect),
        icon_svg("wifi", 18, ""),
        icon_svg("volume", 18, ""),
        icon_svg("settings", 18, "")
    )
}

/// Runtime rendering adds labels in taskbar mode while preserving the click/cycle model.
pub fn configured_html(
    view: &DockView,
    layout: &DockLayout,
    settings: &desilium_config_types::ShellSettings,
    status: &Status,
) -> String {
    let labels = settings.mode == desilium_config_types::PanelMode::Taskbar && settings.show_labels;
    let taskbar = settings.mode == desilium_config_types::PanelMode::Taskbar;
    let radius = if taskbar { TASKBAR_RADIUS } else { 22 };
    let icon = settings.icon_size.saturating_sub(2);
    let strip_width = layout.system.map_or(layout.width as f64, |rect| {
        (rect.x - f64::from(settings.spacing)).max(0.0)
    });
    let mut inner = format!(
        "<div class=dock-item id=dock-launcher data-action=launcher data-tooltip='Applications' style='{}'><span class=dock-launcher>{}</span></div>",
        rect_style(layout.launcher),
        icon_svg("grid", 24, "")
    );
    let tools = [
        ("show-desktop", "Show desktop", "monitor"),
        ("workspace-next", "Next workspace", "layers"),
    ];
    for (rect, (action, tip, glyph)) in layout.tools.iter().zip(tools) {
        inner.push_str(&format!(
            "<div class=dock-item data-action={action} data-tooltip='{tip}' style='{}'><span class=dock-tool>{}</span></div>",
            rect_style(*rect),
            icon_svg(glyph, 20, "")
        ));
    }
    for (index, cell) in view.cells.iter().enumerate() {
        let Some(rect) = layout.items.get(index).copied() else {
            continue;
        };
        if view.separator == Some(index) {
            inner.push_str(&separator(
                rect.x - f64::from(settings.spacing + 5),
                layout.height,
            ));
        }
        let label = if labels {
            format!("<span class=task-label>{}</span>", esc(&cell.name))
        } else {
            String::new()
        };
        let dots = if settings.show_indicators {
            dots_html(&cell.dots)
        } else {
            String::new()
        };
        let focused = if cell.dots.contains(&Dot::Focused) {
            " focused"
        } else {
            ""
        };
        inner.push_str(&format!("<div class='dock-item{focused}' id=dock-{index} data-action=app data-middle-action=app-new data-secondary-action=app-menu data-arg='{}' data-tooltip='{}' style='{}'>{}{label}{dots}</div>",esc(&cell.key),esc(&cell.name),rect_style(rect),icon_html(&cell.icon,"",u32::from(icon))));
    }
    if view.separator == Some(view.cells.len()) {
        let x = layout
            .items
            .last()
            .map_or(layout.launcher.x + layout.launcher.width, |rect| {
                rect.x + rect.width
            })
            + f64::from(settings.spacing);
        inner.push_str(&separator(x, layout.height));
    }
    let system = layout
        .system
        .map_or_else(String::new, |rect| system_html(rect, settings, status));
    let taskbar_css = if taskbar { TASKBAR_CSS } else { "" };
    format!(
        "<style>{taskbar_css}.dock{{position:relative;display:block;padding:0;border:0;box-shadow:inset 0 0 0 1px var(--glass-border);overflow:hidden;border-radius:{radius}px;background:color-mix(in srgb,var(--surface-1) {}%,transparent)}}.dock-strip{{position:absolute;left:0;top:0;height:100%;overflow-x:auto;overflow-y:hidden}}.dock-item{{display:flex;align-items:center;justify-content:center;gap:8px;box-sizing:border-box}}.dock-item .app-icon,.dock-item .app-img,.dock-launcher{{width:{icon}px;height:{icon}px;flex:none}}.task-label{{font-size:11px;min-width:0;max-width:92px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}.task-system{{font-size:12px;white-space:nowrap}}.dock-sep{{position:absolute;width:1px;margin:0}}</style><div class=dock id=dock style='width:{}px;height:{}px'><div class=dock-strip style='width:{strip_width}px'>{inner}</div>{system}</div>",
        settings.opacity, layout.width, layout.height
    )
}
fn rect_style(rect: desilium_ui::Rect) -> String {
    format!(
        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px",
        rect.x, rect.y, rect.width, rect.height
    )
}
fn separator(x: f64, height: u32) -> String {
    format!(
        "<div class=dock-sep style='left:{}px;top:16px;height:{}px'></div>",
        x + 2.0,
        height.saturating_sub(32)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dock::layout::layout, model::icons::GENERIC_TILE};

    fn cell(key: &str, dots: Vec<Dot>) -> DockCell {
        DockCell {
            key: key.into(),
            name: format!("<{key}>"),
            icon: IconSpec::Tile(GENERIC_TILE),
            dots,
        }
    }

    #[test]
    fn cells_carry_escaped_key_name_and_dots() {
        let view = DockView {
            cells: vec![cell("a.desktop", vec![Dot::Open, Dot::Focused])],
            separator: None,
        };
        let html = dock_html(&view, &layout(1, None));
        assert!(html.contains("data-arg=\"a.desktop\""));
        assert!(html.contains("data-tooltip=\"&lt;a.desktop&gt;\""));
        assert_eq!(html.matches("<i").count(), 2);
        assert!(html.contains("<i class=on>"));
    }

    #[test]
    fn separator_is_placed_before_the_marked_cell() {
        let view = DockView {
            cells: vec![cell("a", vec![]), cell("b", vec![])],
            separator: Some(1),
        };
        let html = dock_html(&view, &layout(2, Some(1)));
        let sep = html.find("dock-sep").unwrap();
        assert!(html.find("id=dock-0").unwrap() < sep);
        assert!(sep < html.find("id=dock-1").unwrap());
    }
}
