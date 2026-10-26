use crate::geometry::{Point, Rect};
use crate::ids::WindowId;
use crate::manager::WindowManager;
use crate::state::Layout;

/// A region of the work area that snaps a window when the pointer reaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapZone {
    Left,
    Right,
    /// Top edge; snapping here maximizes the window.
    Maximize,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// The snap zone under `pointer`, or `None` if the pointer is outside `work_area`.
/// Corner zones (both distances below `corner_size`) win over edge zones (below `edge_threshold`).
/// Thresholds are clamped to `[0, min(w, h) / 2]` of the work area.
pub fn snap_zone_at(
    work_area: Rect,
    pointer: Point,
    edge_threshold: i32,
    corner_size: i32,
) -> Option<SnapZone> {
    if !work_area.contains(pointer) {
        return None;
    }
    let edge = clamp_threshold(edge_threshold, work_area);
    let corner = clamp_threshold(corner_size, work_area);
    let from_left = pointer.x.saturating_sub(work_area.loc.x);
    let from_top = pointer.y.saturating_sub(work_area.loc.y);
    let from_right = work_area
        .right()
        .saturating_sub(1)
        .saturating_sub(pointer.x);
    let from_bottom = work_area
        .bottom()
        .saturating_sub(1)
        .saturating_sub(pointer.y);

    let near_top = from_top < corner;
    let near_bottom = from_bottom < corner;
    let near_left = from_left < corner;
    let near_right = from_right < corner;
    if near_top && near_left {
        return Some(SnapZone::TopLeft);
    }
    if near_top && near_right {
        return Some(SnapZone::TopRight);
    }
    if near_bottom && near_left {
        return Some(SnapZone::BottomLeft);
    }
    if near_bottom && near_right {
        return Some(SnapZone::BottomRight);
    }
    if from_top < edge {
        return Some(SnapZone::Maximize);
    }
    if from_left < edge {
        return Some(SnapZone::Left);
    }
    if from_right < edge {
        return Some(SnapZone::Right);
    }
    None
}

fn clamp_threshold(value: i32, work_area: Rect) -> i32 {
    value.max(0).min(work_area.size.w.min(work_area.size.h) / 2)
}

/// The rect a window occupies in `zone`. The first half of an odd dimension is the floor,
/// so the two halves always tile the work area exactly, without gaps or overlap.
pub fn snap_rect(work_area: Rect, zone: SnapZone) -> Rect {
    let width = work_area.size.w.max(0);
    let height = work_area.size.h.max(0);
    let first_w = width / 2;
    let first_h = height / 2;
    let left_x = work_area.loc.x;
    let right_x = left_x.saturating_add(first_w);
    let top_y = work_area.loc.y;
    let bottom_y = top_y.saturating_add(first_h);
    let (x, w) = (left_x, first_w);
    let (x_r, w_r) = (right_x, width - first_w);
    let (y, h) = (top_y, first_h);
    let (y_b, h_b) = (bottom_y, height - first_h);
    match zone {
        SnapZone::Maximize => work_area,
        SnapZone::Left => Rect::new(x, top_y, w, height),
        SnapZone::Right => Rect::new(x_r, top_y, w_r, height),
        SnapZone::TopLeft => Rect::new(x, y, w, h),
        SnapZone::TopRight => Rect::new(x_r, y, w_r, h),
        SnapZone::BottomLeft => Rect::new(x, y_b, w, h_b),
        SnapZone::BottomRight => Rect::new(x_r, y_b, w_r, h_b),
    }
}

/// Target rect to draw as the translucent preview while a snap zone is active.
pub fn snap_preview(work_area: Rect, zone: SnapZone) -> Rect {
    snap_rect(work_area, zone)
}

impl WindowManager {
    /// Snaps a window into `zone`, remembering its pre-snap rect. `Maximize` maximizes it.
    /// Returns true if the window exists.
    pub fn snap_window(&mut self, id: WindowId, zone: SnapZone) -> bool {
        if zone == SnapZone::Maximize {
            return self.maximize(id);
        }
        let rect = snap_rect(self.work_area(), zone);
        self.enter_layout(id, Layout::Snapped, rect)
    }

    /// Puts a window at `rect`, clamped into the work area, like a snap to a custom zone.
    /// Used to restore a saved layout. Returns true if the window exists.
    pub fn place_window(&mut self, id: WindowId, rect: Rect) -> bool {
        let area = self.work_area();
        let width = rect.size.w.clamp(1, area.size.w.max(1));
        let height = rect.size.h.clamp(1, area.size.h.max(1));
        let x = rect.loc.x.clamp(area.loc.x, (area.right() - width).max(area.loc.x));
        let y = rect.loc.y.clamp(area.loc.y, (area.bottom() - height).max(area.loc.y));
        self.enter_layout(id, Layout::Snapped, Rect::new(x, y, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;

    fn odd_area() -> Rect {
        Rect::new(0, 0, 101, 51)
    }

    #[test]
    fn snap_table_for_odd_sizes() {
        let cases = [
            (SnapZone::Left, Rect::new(0, 0, 50, 51)),
            (SnapZone::Right, Rect::new(50, 0, 51, 51)),
            (SnapZone::Maximize, Rect::new(0, 0, 101, 51)),
            (SnapZone::TopLeft, Rect::new(0, 0, 50, 25)),
            (SnapZone::TopRight, Rect::new(50, 0, 51, 25)),
            (SnapZone::BottomLeft, Rect::new(0, 25, 50, 26)),
            (SnapZone::BottomRight, Rect::new(50, 25, 51, 26)),
        ];
        for (zone, expected) in cases {
            assert_eq!(snap_rect(odd_area(), zone), expected, "{zone:?}");
        }
    }

    #[test]
    fn halves_tile_without_gap_or_overlap() {
        let area = Rect::new(7, 3, 101, 51);
        let left = snap_rect(area, SnapZone::Left);
        let right = snap_rect(area, SnapZone::Right);
        assert_eq!(left.right(), right.loc.x);
        assert_eq!(left.size.w + right.size.w, area.size.w);
        let top = snap_rect(area, SnapZone::TopLeft);
        let bottom = snap_rect(area, SnapZone::BottomLeft);
        assert_eq!(top.bottom(), bottom.loc.y);
        assert_eq!(top.size.h + bottom.size.h, area.size.h);
    }

    #[test]
    fn placed_windows_stay_inside_the_work_area() {
        let mut manager = WindowManager::new(Rect::new(0, 0, 800, 600));
        let id = manager.open_window(Size { w: 100, h: 100 });
        assert!(manager.place_window(id, Rect::new(700, 500, 400, 300)));
        assert_eq!(manager.rect(id), Some(Rect::new(400, 300, 400, 300)));
        assert!(manager.place_window(id, Rect::new(-50, -50, 5000, 5000)));
        assert_eq!(manager.rect(id), Some(Rect::new(0, 0, 800, 600)));
    }

    #[test]
    fn snap_preview_matches_snap_rect() {
        assert_eq!(
            snap_preview(odd_area(), SnapZone::Right),
            snap_rect(odd_area(), SnapZone::Right)
        );
    }

    #[test]
    fn zone_table_for_edges_and_corners() {
        let area = Rect::new(0, 0, 1000, 800);
        let cases = [
            ((5, 400), Some(SnapZone::Left)),
            ((994, 400), Some(SnapZone::Right)),
            ((500, 3), Some(SnapZone::Maximize)),
            ((5, 5), Some(SnapZone::TopLeft)),
            ((990, 5), Some(SnapZone::TopRight)),
            ((5, 790), Some(SnapZone::BottomLeft)),
            ((995, 795), Some(SnapZone::BottomRight)),
            ((500, 400), None),
            ((-1, 5), None),
            ((1000, 5), None),
        ];
        for ((x, y), expected) in cases {
            let pointer = Point { x, y };
            assert_eq!(snap_zone_at(area, pointer, 10, 50), expected, "{pointer:?}");
        }
    }

    #[test]
    fn corners_win_over_edges() {
        let area = Rect::new(0, 0, 1000, 800);
        let corner = snap_zone_at(area, Point { x: 2, y: 2 }, 10, 50);
        assert_eq!(corner, Some(SnapZone::TopLeft));
    }

    #[test]
    fn zero_thresholds_never_snap() {
        let area = Rect::new(0, 0, 1000, 800);
        assert_eq!(snap_zone_at(area, Point { x: 0, y: 0 }, 0, 0), None);
    }

    #[test]
    fn negative_thresholds_are_treated_as_zero() {
        let area = Rect::new(0, 0, 1000, 800);
        assert_eq!(snap_zone_at(area, Point { x: 0, y: 400 }, -5, -5), None);
    }

    #[test]
    fn huge_threshold_is_clamped_to_half_the_smaller_side() {
        let area = Rect::new(0, 0, 1000, 800);
        // Clamped to 400, so y = 399 is still inside the top edge zone.
        let zone = snap_zone_at(area, Point { x: 500, y: 399 }, 10_000, 0);
        assert_eq!(zone, Some(SnapZone::Maximize));
        let below = snap_zone_at(area, Point { x: 500, y: 400 }, 10_000, 0);
        assert_eq!(below, None);
    }

    #[test]
    fn zone_is_relative_to_work_area_origin() {
        let area = Rect::new(100, 200, 1000, 800);
        assert_eq!(
            snap_zone_at(area, Point { x: 103, y: 500 }, 10, 50),
            Some(SnapZone::Left)
        );
    }

    #[test]
    fn snap_window_left_remembers_pre_snap_rect() {
        let mut wm = WindowManager::new(Rect::new(0, 0, 1000, 800));
        let id = wm.open_window(Size { w: 200, h: 100 });
        wm.move_to(id, Point { x: 100, y: 100 });
        assert!(wm.snap_window(id, SnapZone::Left));
        assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 500, 800)));
        assert_eq!(wm.window_state(id), Some(crate::WindowState::Normal));
        assert_eq!(wm.restore_rect(id), Some(Rect::new(100, 100, 200, 100)));
    }

    #[test]
    fn snap_then_maximize_then_unmaximize_returns_to_pre_snap_rect() {
        let mut wm = WindowManager::new(Rect::new(0, 0, 1000, 800));
        let id = wm.open_window(Size { w: 200, h: 100 });
        wm.move_to(id, Point { x: 100, y: 100 });
        wm.snap_window(id, SnapZone::TopRight);
        assert!(wm.snap_window(id, SnapZone::Maximize));
        assert_eq!(wm.window_state(id), Some(crate::WindowState::Maximized));
        assert_eq!(wm.restore_rect(id), Some(Rect::new(100, 100, 200, 100)));
        wm.unmaximize(id);
        assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
    }

    #[test]
    fn snap_window_unknown_returns_false() {
        let mut wm = WindowManager::new(Rect::new(0, 0, 10, 10));
        let stray = WindowManager::new(Rect::new(0, 0, 10, 10)).open_window(Size { w: 1, h: 1 });
        assert!(!wm.snap_window(stray, SnapZone::Left));
    }
}
