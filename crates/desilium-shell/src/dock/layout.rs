//! Geometry of the dock tile. The CSS in `assets/dock.css` uses the same numbers.

use desilium_ui::Rect;

/// Icon cell edge.
pub const ITEM: u32 = 48;
/// Space between cells.
pub const GAP: u32 = 8;
/// Horizontal padding inside the tile.
pub const PAD_X: u32 = 10;
/// Vertical padding inside the tile.
pub const PAD_Y: u32 = 7;
/// Tile border width.
pub const BORDER: u32 = 1;
/// Separator width including its 2 px side margins.
pub const SEPARATOR: u32 = 5;
/// Tile height.
pub const HEIGHT: u32 = ITEM + 2 * (PAD_Y + BORDER);
/// Corner radius of the tile (also the compositor's clip radius).
pub const RADIUS: u32 = 22;
/// Distance between the tile and the screen edge.
pub const BOTTOM_MARGIN: i32 = 14;

/// Where everything sits inside the dock surface.
#[derive(Debug, Clone, PartialEq)]
pub struct DockLayout {
    /// Surface width.
    pub width: u32,
    /// The launcher button.
    pub launcher: Rect,
    /// App cells in order.
    pub items: Vec<Rect>,
}

fn cell(x: u32) -> Rect {
    Rect {
        x: f64::from(x),
        y: f64::from(BORDER + PAD_Y),
        width: f64::from(ITEM),
        height: f64::from(ITEM),
    }
}

/// Lays out the launcher button, `count` app cells and an optional separator before cell `separator`.
pub fn layout(count: usize, separator: Option<usize>) -> DockLayout {
    let mut x = BORDER + PAD_X;
    let launcher = cell(x);
    x += ITEM;
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        x += GAP;
        if separator == Some(index) {
            x += SEPARATOR + GAP;
        }
        items.push(cell(x));
        x += ITEM;
    }
    if separator == Some(count) {
        x += GAP + SEPARATOR;
    }
    DockLayout {
        width: x + PAD_X + BORDER,
        launcher,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dock_is_just_the_launcher() {
        let l = layout(0, None);
        assert_eq!(l.width, 1 + 10 + 48 + 10 + 1);
        assert!(l.items.is_empty());
    }

    #[test]
    fn cells_are_spaced_by_the_gap() {
        let l = layout(3, None);
        assert_eq!(l.items[0].x, 11.0 + 48.0 + 8.0);
        assert_eq!(l.items[1].x - l.items[0].x, 56.0);
        assert_eq!(l.width, 11 + 4 * 48 + 3 * 8 + 11);
    }

    #[test]
    fn separator_adds_its_width_and_gap() {
        let plain = layout(3, None);
        let split = layout(3, Some(1));
        assert_eq!(split.width, plain.width + SEPARATOR + GAP);
        assert_eq!(split.items[0].x, plain.items[0].x);
        assert_eq!(
            split.items[1].x,
            plain.items[1].x + f64::from(SEPARATOR + GAP)
        );
    }

    #[test]
    fn height_matches_the_mockup() {
        assert_eq!(HEIGHT, 64);
    }
}
