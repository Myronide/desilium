//! The HTML of the desktop icons, their menus and gestures.

use super::{Desktop, Gesture, Menu};
use crate::{
    model::{
        desktop::{self, DeskItem, ICON_PX, Kind},
        grid::{self, Cell, Grid, Rect},
    },
    view::{
        esc,
        icon::{IconSpec, icon_html},
    },
};

/// Width of a context menu; `desktop.css` uses the same.
const MENU_WIDTH: i32 = 210;
/// Height of one menu row plus the gap, and the padding around all rows.
const MENU_ROW: i32 = 36;
const MENU_PADDING: i32 = 12;
/// Gap kept between a menu and the screen edge.
const MENU_EDGE: i32 = 8;
/// Space around an icon inside its cell.
const CELL_PADDING: i32 = 4;

/// Where `menu` goes so that it stays on a surface of `size`, with `rows` commands.
pub fn menu_origin(menu: &Menu, rows: usize, size: (u32, u32)) -> (i32, i32) {
    let width = i32::try_from(size.0).unwrap_or(i32::MAX);
    let height = i32::try_from(size.1).unwrap_or(i32::MAX);
    let menu_height = i32::try_from(rows).unwrap_or(0) * MENU_ROW + MENU_PADDING;
    let x = menu.at.0.min(width - MENU_WIDTH - MENU_EDGE).max(MENU_EDGE);
    let y = menu
        .at
        .1
        .min(height - menu_height - MENU_EDGE)
        .max(MENU_EDGE);
    (x, y)
}

fn style(rect: Rect) -> String {
    format!(
        "left:{}px;top:{}px;width:{}px;height:{}px",
        rect.0, rect.1, rect.2, rect.3
    )
}

/// One icon: its cell, shifted by `shift` while it is carried.
fn cell_html(
    index: usize,
    item: &DeskItem,
    icon: &IconSpec,
    rect: Rect,
    shift: Option<(i32, i32)>,
    desktop: &Desktop,
) -> String {
    let mut classes = String::from("dicon");
    if desktop.selected.contains(&item.file_name) {
        classes.push_str(" sel");
    }
    if item.kind == (Kind::Launcher { trusted: false }) {
        classes.push_str(" untrusted");
    }
    let renaming = desktop.renaming.as_deref() == Some(item.file_name.as_str());
    if shift.is_some() {
        classes.push_str(" carried");
    }
    let (dx, dy) = shift.unwrap_or_default();
    let rect = (rect.0 + dx, rect.1 + dy, rect.2, rect.3);
    let label = if renaming {
        format!(
            "<input id=desk-rename class=drename value='{}' data-submit=desk-rename>",
            esc(&item.file_name)
        )
    } else {
        format!(
            "<div class=dname style='max-width:{}px'>{}</div>",
            (rect.2 - 2 * CELL_PADDING).max(1),
            esc(&desktop::short_label(&item.name))
        )
    };
    // The rename field must keep its own mouse handling, so that cell cannot be dragged.
    let drag = if renaming {
        String::new()
    } else {
        format!("data-drag=desk-icon data-arg={index}")
    };
    format!(
        "<div class='{classes}' data-action=desk-item data-secondary-action=desk-item-menu {drag} data-tooltip='{}' style='{}'><div class=dthumb>{}</div>{label}</div>",
        esc(&item.name),
        style(rect),
        icon_html(icon, "", ICON_PX)
    )
}

fn menu_html(desktop: &Desktop, menu: &Menu, size: (u32, u32)) -> String {
    let commands = desktop.commands(menu.target.as_deref());
    let (x, y) = menu_origin(menu, commands.len(), size);
    let rows: String = commands
        .iter()
        .map(|command| {
            format!(
                "<button class=dcmd data-action=desk-cmd data-arg={}>{}</button>",
                command.name(),
                command.label()
            )
        })
        .collect();
    format!("<div class=dmenu style='left:{x}px;top:{y}px'>{rows}</div>")
}

/// The content of the desktop layer: one cell per icon, the drop target or selection rectangle
/// while one is drawn, the open menu, the last problem.
///
/// `icons` and `cells` have one entry per item. `size` is the surface.
pub fn html(
    desktop: &Desktop,
    icons: &[IconSpec],
    grid: &Grid,
    cells: &[Cell],
    size: (u32, u32),
) -> String {
    let carried = match &desktop.gesture {
        Some(Gesture::Icons {
            origin, pointer, ..
        }) => Some((
            (pointer.0 - origin.0).round() as i32,
            (pointer.1 - origin.1).round() as i32,
        )),
        _ => None,
    };
    let mut out = String::new();
    if let Some(Gesture::Icons { target, .. }) = &desktop.gesture {
        out.push_str(&format!(
            "<div class=dtarget style='{}'></div>",
            style(grid.rect(*target))
        ));
    }
    for (index, ((item, icon), cell)) in desktop.items.iter().zip(icons).zip(cells).enumerate() {
        let shift = carried.filter(|_| desktop.selected.contains(&item.file_name));
        out.push_str(&cell_html(
            index,
            item,
            icon,
            grid.rect(*cell),
            shift,
            desktop,
        ));
    }
    if let Some(Gesture::Band { from, to }) = &desktop.gesture {
        out.push_str(&format!(
            "<div class=dband style='{}'></div>",
            style(grid::spanned(*from, *to))
        ));
    }
    if let Some(menu) = &desktop.menu {
        out.push_str(&menu_html(desktop, menu, size));
    }
    if !desktop.notice.is_empty() {
        out.push_str(&format!(
            "<div class=dnotice><span>{}</span></div>",
            esc(&desktop.notice)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, path::PathBuf};

    fn item(name: &str, kind: Kind) -> DeskItem {
        DeskItem {
            path: PathBuf::from(format!("/d/{name}")),
            file_name: name.to_owned(),
            name: name.to_owned(),
            kind,
            icon: None,
        }
    }

    fn setup() -> (Desktop, Grid) {
        let mut desktop = Desktop::default();
        desktop.set_items(vec![
            item("<b>Docs</b>", Kind::Folder),
            item("x.desktop", Kind::Launcher { trusted: false }),
        ]);
        (desktop, Grid::new((800, 600), 32, 100))
    }

    fn render(desktop: &mut Desktop, grid: &Grid) -> String {
        let cells = desktop.cells(grid);
        html(
            desktop,
            &[IconSpec::NEUTRAL, IconSpec::NEUTRAL],
            grid,
            &cells,
            (800, 600),
        )
    }

    #[test]
    fn every_item_is_a_cell_sized_box_with_its_index() {
        let (mut desktop, grid) = setup();
        let html = render(&mut desktop, &grid);
        let (x, y, w, h) = grid.rect((0, 0));
        assert!(html.contains(&format!("left:{x}px;top:{y}px;width:{w}px;height:{h}px")));
        assert!(html.contains("data-drag=desk-icon data-arg=0"));
        assert!(html.contains("data-drag=desk-icon data-arg=1"));
        assert_eq!(html.matches("data-action=desk-item ").count(), 2);
    }

    #[test]
    fn names_are_escaped_everywhere() {
        let (mut desktop, grid) = setup();
        let html = render(&mut desktop, &grid);
        assert!(!html.contains("<b>Docs"), "{html}");
        assert!(html.contains("&lt;b&gt;Docs"));
    }

    #[test]
    fn selection_untrusted_launchers_and_rename_are_visible_in_the_markup() {
        let (mut desktop, grid) = setup();
        desktop.selected = BTreeSet::from(["x.desktop".to_owned()]);
        let html = render(&mut desktop, &grid);
        assert!(html.contains("dicon sel untrusted"));
        desktop.renaming = Some("x.desktop".into());
        let html = render(&mut desktop, &grid);
        assert!(html.contains("id=desk-rename") && html.contains("data-submit=desk-rename"));
        assert!(html.contains("value='x.desktop'"));
        assert!(
            !html.contains("data-drag=desk-icon data-arg=1"),
            "the icon being renamed cannot be dragged"
        );
    }

    #[test]
    fn a_menu_lists_its_commands_and_a_notice_shows() {
        let (mut desktop, grid) = setup();
        desktop.menu = Some(Menu {
            target: None,
            at: (100, 100),
        });
        desktop.notice = "no <gio>".into();
        let html = render(&mut desktop, &grid);
        assert!(html.contains("class=dcmd data-action=desk-cmd data-arg=new-folder"));
        assert!(html.contains("Open desktop folder"));
        assert!(html.contains("no &lt;gio&gt;"));
    }

    #[test]
    fn carried_icons_follow_the_pointer_and_mark_the_target_cell() {
        let (mut desktop, grid) = setup();
        desktop.cells(&grid);
        desktop.begin_icon_drag(0, (20.0, 60.0), &grid);
        desktop.drag_icons((220.0, 180.0), &grid);
        let html = render(&mut desktop, &grid);
        let (x, y, ..) = grid.rect((0, 0));
        assert!(html.contains("carried"));
        assert!(
            html.contains(&format!("left:{}px;top:{}px", x + 200, y + 120)),
            "{html}"
        );
        assert!(html.contains("class=dtarget"));
        let others = html.matches("carried").count();
        assert_eq!(others, 1, "only the selected icon is carried");
    }

    #[test]
    fn the_selection_rectangle_is_drawn_between_its_corners() {
        let (mut desktop, grid) = setup();
        desktop.begin_band((300.0, 200.0));
        desktop.drag_band((100.0, 250.0), &grid);
        let html = render(&mut desktop, &grid);
        assert!(html.contains("class=dband style='left:100px;top:200px;width:200px;height:50px'"));
    }

    #[test]
    fn menus_stay_on_the_surface() {
        let menu = |x, y| Menu {
            target: None,
            at: (x, y),
        };
        assert_eq!(menu_origin(&menu(100, 100), 4, (800, 600)), (100, 100));
        let (x, y) = menu_origin(&menu(790, 590), 4, (800, 600));
        assert_eq!(x, 800 - MENU_WIDTH - MENU_EDGE);
        assert_eq!(y, 600 - (4 * MENU_ROW + MENU_PADDING) - MENU_EDGE);
        assert_eq!(
            menu_origin(&menu(-5, -5), 4, (800, 600)),
            (MENU_EDGE, MENU_EDGE)
        );
        let tiny = menu_origin(&menu(10, 10), 4, (100, 100));
        assert_eq!(
            tiny,
            (MENU_EDGE, MENU_EDGE),
            "a tiny surface does not panic"
        );
    }
}
