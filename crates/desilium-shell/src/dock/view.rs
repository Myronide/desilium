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

/// Runtime rendering adds labels in taskbar mode while preserving the click/cycle model.
pub fn configured_html(
    view: &DockView,
    layout: &DockLayout,
    settings: &desilium_config_types::ShellSettings,
    status: &str,
) -> String {
    let labels = settings.mode == desilium_config_types::PanelMode::Taskbar && settings.show_labels;
    let radius = if settings.mode == desilium_config_types::PanelMode::Taskbar {
        8
    } else {
        22
    };
    let icon = settings.icon_size.saturating_sub(2);
    let strip_width = layout.system.map_or(layout.width as f64, |rect| {
        (rect.x - f64::from(settings.spacing)).max(0.0)
    });
    let mut inner = format!(
        "<div class=dock-item id=dock-launcher data-action=launcher data-tooltip='Applications' style='{}'><span class=dock-launcher>{}</span></div>",
        rect_style(layout.launcher),
        icon_svg("grid", 24, "")
    );
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
        inner.push_str(&format!("<div class=dock-item id=dock-{index} data-action=app data-middle-action=app-new data-secondary-action=app-menu data-arg='{}' data-tooltip='{}' style='{}'>{}{label}{dots}</div>",esc(&cell.key),esc(&cell.name),rect_style(rect),icon_html(&cell.icon,"",u32::from(icon))));
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
    let system=layout.system.map_or_else(String::new,|rect|format!("<button class=task-system data-action={} style='{}' data-tooltip='Quick settings'>{} ▾</button>",if settings.quick_settings{"quick-settings"}else{"none"},rect_style(rect),esc(status)));
    format!(
        "<style>.dock{{position:relative;display:block;padding:0;border:0;box-shadow:inset 0 0 0 1px var(--glass-border);overflow:hidden;border-radius:{radius}px;background:color-mix(in srgb,var(--surface-1) {}%,transparent)}}.dock-strip{{position:absolute;left:0;top:0;height:100%;overflow-x:auto;overflow-y:hidden}}.dock-item{{display:flex;align-items:center;justify-content:center;gap:8px;box-sizing:border-box}}.dock-item .app-icon,.dock-item .app-img,.dock-launcher{{width:{icon}px;height:{icon}px;flex:none}}.task-label{{font-size:11px;min-width:0;max-width:92px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}.task-system{{display:flex;align-items:center;justify-content:center;font-size:12px;background:var(--surface-3);border-radius:8px;white-space:nowrap;cursor:pointer}}.dock-sep{{position:absolute;width:1px;margin:0}}</style><div class=dock id=dock style='width:{}px;height:{}px'><div class=dock-strip style='width:{strip_width}px'>{inner}</div>{system}</div>",
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
