//! The desktop's icon grid: it is fitted to the free area of the screen, icons sit in cells.
//!
//! The grid is recomputed from the surface size and the space the panels take, so it always fills
//! the free area exactly: the cell size is the area divided by the whole number of cells that
//! fit, never a fixed size with a ragged remainder. A cell is a `(column, row)` pair.

use std::collections::{BTreeMap, HashSet};

/// Smallest width a cell may get; the column count is the area width divided by this.
pub const MIN_CELL_WIDTH: i32 = 88;
/// Smallest height a cell may get.
pub const MIN_CELL_HEIGHT: i32 = 96;
/// Space between the screen edges, the panels and the grid.
pub const MARGIN: i32 = 12;

/// A grid cell as `(column, row)`.
pub type Cell = (u32, u32);

/// A rectangle `(x, y, width, height)` in surface pixels.
pub type Rect = (i32, i32, i32, i32);

/// The grid for one surface size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    /// Columns that fit the width.
    pub cols: u32,
    /// Rows that fit the height.
    pub rows: u32,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl Grid {
    /// The grid for a surface of `size`, with `top` and `bottom` pixels taken by the panels.
    /// At least one column and one row always exist, so a tiny surface still has a grid.
    pub fn new(size: (u32, u32), top: u32, bottom: u32) -> Self {
        let surface = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
        let left = MARGIN;
        let top = surface(top).saturating_add(MARGIN);
        let width = (surface(size.0) - 2 * MARGIN).max(1);
        let height = (surface(size.1) - top - surface(bottom) - MARGIN).max(1);
        let count =
            |extent: i32, minimum: i32| u32::try_from((extent / minimum).max(1)).unwrap_or(1);
        Self {
            cols: count(width, MIN_CELL_WIDTH),
            rows: count(height, MIN_CELL_HEIGHT),
            left,
            top,
            width,
            height,
        }
    }

    /// Whether `cell` is on the screen.
    pub fn contains(&self, cell: Cell) -> bool {
        cell.0 < self.cols && cell.1 < self.rows
    }

    /// The pixels of `cell`. Neighbouring cells share their edge exactly, so nothing is lost to
    /// rounding. Cells right of the last column continue at the same pitch (off screen).
    pub fn rect(&self, cell: Cell) -> Rect {
        let cols = i64::from(self.cols);
        let rows = i64::from(self.rows);
        let edge = |index: i64, count: i64, extent: i32, origin: i32| {
            origin + i32::try_from(index * i64::from(extent) / count).unwrap_or(i32::MAX)
        };
        let (column, row) = (i64::from(cell.0), i64::from(cell.1));
        let x0 = edge(column, cols, self.width, self.left);
        let x1 = edge(column + 1, cols, self.width, self.left);
        let y0 = edge(row, rows, self.height, self.top);
        let y1 = edge(row + 1, rows, self.height, self.top);
        (x0, y0, x1 - x0, y1 - y0)
    }

    /// The cell a point lies in; points outside the grid belong to the nearest edge cell.
    pub fn cell_at(&self, x: f64, y: f64) -> Cell {
        let index = |value: f64, origin: i32, extent: i32, count: u32| {
            let relative = (value - f64::from(origin)) / f64::from(extent) * f64::from(count);
            (relative.floor().max(0.0) as u32).min(count - 1)
        };
        (
            index(x, self.left, self.width, self.cols),
            index(y, self.top, self.height, self.rows),
        )
    }

    /// Cells in the order new icons fill them: down a column, then the next column. Goes on past
    /// the right edge so that more icons than cells still get a place.
    fn fill_order(&self) -> impl Iterator<Item = Cell> + use<> {
        let rows = self.rows;
        (0u32..).flat_map(move |column| (0..rows).map(move |row| (column, row)))
    }

    /// The free cell nearest to `target` (by steps along columns and rows; ties go to the lower
    /// column, then the lower row). When the screen is full, the first free cell past it.
    pub fn nearest_free(&self, target: Cell, taken: &HashSet<Cell>) -> Cell {
        let target = (target.0.min(self.cols - 1), target.1.min(self.rows - 1));
        let mut best: Option<(u32, Cell)> = None;
        for column in 0..self.cols {
            for row in 0..self.rows {
                let cell = (column, row);
                if taken.contains(&cell) {
                    continue;
                }
                let distance = column.abs_diff(target.0) + row.abs_diff(target.1);
                if best.is_none_or(|(known, _)| distance < known) {
                    best = Some((distance, cell));
                }
            }
        }
        best.map(|(_, cell)| cell).unwrap_or_else(|| {
            self.fill_order()
                .find(|cell| !taken.contains(cell))
                .unwrap_or(target)
        })
    }

    /// Cells for `names`, in order. A name with a saved cell that is on the screen and not wanted
    /// by an earlier name keeps it; the others take the first free cells, down the columns.
    ///
    /// A saved cell that is off the screen (the screen got smaller) is kept in `saved` by the
    /// caller; the icon only sits somewhere else for now.
    pub fn place(&self, names: &[&str], saved: &BTreeMap<String, Cell>) -> Vec<Cell> {
        let mut taken: HashSet<Cell> = HashSet::new();
        let mut cells: Vec<Option<Cell>> = names
            .iter()
            .map(|name| {
                let cell = saved
                    .get(*name)
                    .copied()
                    .filter(|cell| self.contains(*cell))?;
                taken.insert(cell).then_some(cell)
            })
            .collect();
        let mut free = self.fill_order().filter(|cell| !taken.contains(cell));
        for slot in &mut cells {
            if slot.is_none() {
                *slot = free.next();
            }
        }
        cells
            .into_iter()
            .map(|cell| cell.unwrap_or((0, 0)))
            .collect()
    }

    /// Moves the icons `moving` so that `anchor` lands on `target` and the others keep their
    /// distance to it, clamped to the screen. An icon whose new cell is taken by one that stays
    /// goes to the nearest free cell. `cells` holds where every icon is now.
    pub fn move_group(
        &self,
        cells: &mut BTreeMap<String, Cell>,
        moving: &[String],
        anchor: &str,
        target: Cell,
    ) {
        let Some(&start) = cells.get(anchor) else {
            return;
        };
        let delta = (
            i64::from(target.0) - i64::from(start.0),
            i64::from(target.1) - i64::from(start.1),
        );
        let mut order: Vec<&String> = moving.iter().collect();
        // The icon the user holds gets its wish first.
        order.sort_by_key(|name| name.as_str() != anchor);
        let from: Vec<(String, Cell)> = order
            .iter()
            .filter_map(|name| cells.get(*name).map(|cell| ((*name).clone(), *cell)))
            .collect();
        for (name, _) in &from {
            cells.remove(name);
        }
        let mut taken: HashSet<Cell> = cells.values().copied().collect();
        let clamp =
            |value: i64, count: u32| u32::try_from(value.max(0)).unwrap_or(0).min(count - 1);
        for (name, cell) in from {
            let wanted = (
                clamp(i64::from(cell.0) + delta.0, self.cols),
                clamp(i64::from(cell.1) + delta.1, self.rows),
            );
            let chosen = if taken.contains(&wanted) {
                self.nearest_free(wanted, &taken)
            } else {
                wanted
            };
            taken.insert(chosen);
            cells.insert(name, chosen);
        }
    }
}

/// Whether two rectangles overlap (touching edges do not count).
pub fn intersects(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// The rectangle between two corners, whichever way the pointer moved.
pub fn spanned(from: (f64, f64), to: (f64, f64)) -> Rect {
    let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
    let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
    (x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        // 1280 x 720, bar 32 above, dock 100 below.
        Grid::new((1280, 720), 32, 100)
    }

    #[test]
    fn the_grid_fills_the_free_area_exactly() {
        let grid = grid();
        // Width 1280 - 24 = 1256 -> 14 columns; height 720 - 44 - 100 - 12 = 564 -> 5 rows.
        assert_eq!((grid.cols, grid.rows), (14, 5));
        let first = grid.rect((0, 0));
        let last = grid.rect((grid.cols - 1, grid.rows - 1));
        assert_eq!((first.0, first.1), (MARGIN, 32 + MARGIN));
        assert_eq!(
            last.0 + last.2,
            1280 - MARGIN,
            "the last column ends at the right margin"
        );
        assert_eq!(
            last.1 + last.3,
            720 - 100 - MARGIN,
            "the last row ends above the dock"
        );
    }

    #[test]
    fn neighbouring_cells_share_their_edges_without_gaps_or_overlaps() {
        for size in [
            (1280, 720),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
            (801, 599),
        ] {
            let grid = Grid::new(size, 32, 90);
            for column in 0..grid.cols - 1 {
                let (a, b) = (grid.rect((column, 0)), grid.rect((column + 1, 0)));
                assert_eq!(a.0 + a.2, b.0, "{size:?} column {column}");
            }
            for row in 0..grid.rows - 1 {
                let (a, b) = (grid.rect((0, row)), grid.rect((0, row + 1)));
                assert_eq!(a.1 + a.3, b.1, "{size:?} row {row}");
            }
            let cell = grid.rect((0, 0));
            assert!(
                cell.2 >= MIN_CELL_WIDTH && cell.3 >= MIN_CELL_HEIGHT || grid.cols * grid.rows == 1
            );
        }
    }

    #[test]
    fn a_bigger_screen_gets_more_cells_not_bigger_gaps() {
        let small = Grid::new((1280, 720), 32, 100);
        let large = Grid::new((2560, 1440), 32, 100);
        assert!(large.cols > small.cols && large.rows > small.rows);
        assert!(large.rect((0, 0)).2 < MIN_CELL_WIDTH * 2);
    }

    #[test]
    fn a_tiny_surface_still_has_one_cell() {
        let grid = Grid::new((10, 10), 32, 100);
        assert_eq!((grid.cols, grid.rows), (1, 1));
        assert!(grid.rect((0, 0)).2 > 0);
    }

    #[test]
    fn points_map_to_cells_and_outside_points_to_the_nearest_edge_cell() {
        let grid = grid();
        let (x, y, w, h) = grid.rect((3, 2));
        assert_eq!(
            grid.cell_at(f64::from(x + w / 2), f64::from(y + h / 2)),
            (3, 2)
        );
        assert_eq!(grid.cell_at(-50.0, -50.0), (0, 0));
        assert_eq!(grid.cell_at(5000.0, 5000.0), (grid.cols - 1, grid.rows - 1));
    }

    #[test]
    fn new_icons_fill_columns_top_down() {
        let grid = grid();
        let cells = grid.place(&["a", "b", "c", "d", "e", "f"], &BTreeMap::new());
        assert_eq!(cells, [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0)]);
    }

    #[test]
    fn saved_cells_are_kept_and_the_rest_fill_the_gaps() {
        let grid = grid();
        let saved = BTreeMap::from([("b".to_owned(), (0, 0)), ("c".to_owned(), (5, 3))]);
        let cells = grid.place(&["a", "b", "c", "d"], &saved);
        assert_eq!(cells, [(0, 1), (0, 0), (5, 3), (0, 2)]);
    }

    #[test]
    fn a_cell_wanted_twice_goes_to_the_first_and_cells_off_screen_are_replaced() {
        let grid = grid();
        let saved = BTreeMap::from([
            ("a".to_owned(), (2, 2)),
            ("b".to_owned(), (2, 2)),
            ("c".to_owned(), (99, 0)),
        ]);
        let cells = grid.place(&["a", "b", "c"], &saved);
        assert_eq!(cells[0], (2, 2));
        assert_ne!(cells[1], (2, 2));
        assert!(grid.contains(cells[2]));
        let unique: HashSet<_> = cells.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn more_icons_than_cells_continue_past_the_edge_without_sharing_a_cell() {
        let grid = Grid::new((300, 300), 0, 0);
        let names: Vec<String> = (0..20).map(|i| format!("f{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let cells = grid.place(&refs, &BTreeMap::new());
        let unique: HashSet<_> = cells.iter().collect();
        assert_eq!(unique.len(), 20);
        assert!(cells.iter().any(|cell| !grid.contains(*cell)));
    }

    #[test]
    fn the_nearest_free_cell_breaks_ties_towards_the_lower_column_then_row() {
        let grid = grid();
        let taken = HashSet::from([(3, 3)]);
        assert_eq!(grid.nearest_free((3, 3), &taken), (2, 3));
        let around = HashSet::from([(3, 3), (3, 2), (2, 3), (4, 3), (3, 4)]);
        assert_eq!(grid.nearest_free((3, 3), &around), (1, 3));
        assert_eq!(grid.nearest_free((0, 0), &HashSet::new()), (0, 0));
    }

    #[test]
    fn dropping_on_a_taken_cell_uses_the_nearest_free_one() {
        let grid = grid();
        let mut cells = BTreeMap::from([("a".to_owned(), (0, 0)), ("b".to_owned(), (4, 2))]);
        grid.move_group(&mut cells, &["a".to_owned()], "a", (4, 2));
        assert_eq!(cells["b"], (4, 2), "the icon that stayed is untouched");
        assert_eq!(cells["a"], (3, 2), "the nearest free neighbour");
    }

    #[test]
    fn a_group_keeps_its_shape_and_is_clamped_to_the_screen() {
        let grid = grid();
        let mut cells = BTreeMap::from([
            ("a".to_owned(), (1, 1)),
            ("b".to_owned(), (2, 1)),
            ("c".to_owned(), (1, 2)),
        ]);
        let moving: Vec<String> = ["a", "b", "c"].map(str::to_owned).to_vec();
        grid.move_group(&mut cells, &moving, "a", (5, 1));
        assert_eq!(
            (cells["a"], cells["b"], cells["c"]),
            ((5, 1), (6, 1), (5, 2))
        );
        grid.move_group(&mut cells, &moving, "a", (grid.cols - 1, 0));
        assert_eq!(cells["a"], (grid.cols - 1, 0));
        assert!(
            grid.contains(cells["b"]) && grid.contains(cells["c"]),
            "the rest of the group stays on the screen"
        );
        let unique: HashSet<_> = cells.values().collect();
        assert_eq!(unique.len(), 3, "no two icons share a cell");
    }

    #[test]
    fn rectangles_intersect_only_when_they_overlap() {
        assert!(intersects((0, 0, 10, 10), (5, 5, 10, 10)));
        assert!(
            !intersects((0, 0, 10, 10), (10, 0, 10, 10)),
            "touching is not overlapping"
        );
        assert_eq!(spanned((30.0, 40.0), (10.0, 50.0)), (10, 40, 20, 10));
    }
}
