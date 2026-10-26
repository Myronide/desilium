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
        "<div class=dock-item id=dock-{index} data-action=app data-middle-action=app-new data-arg=\"{key}\" data-tooltip=\"{name}\">{icon}{dots}</div>",
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
