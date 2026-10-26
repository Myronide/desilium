//! Drag-to-snap: the translucent preview that follows a title bar drag near a screen edge,
//! and the snap that happens when the button is released.

use crate::state::DesiliumState;
use desilium_wm::{Point, Rect, SnapZone, WindowId, WindowState, snap_preview, snap_zone_at};

/// Pointer distance from a work area edge that arms the edge zones, in logical pixels.
pub const EDGE_BAND: i32 = 8;
/// Side of the square at each work area corner that arms a quarter zone, in logical pixels.
pub const CORNER_ZONE: i32 = 48;

/// The zone under the pointer and the rect the window would land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapPreview {
    pub zone: SnapZone,
    pub rect: Rect,
}

/// Preview for a pointer at `pointer` during a move drag; `None` outside every zone or when
/// the window is not being dragged.
pub fn preview_for_pointer(work_area: Rect, pointer: Point, dragging: bool) -> Option<SnapPreview> {
    if !dragging {
        return None;
    }
    let zone = snap_zone_at(work_area, pointer, EDGE_BAND, CORNER_ZONE)?;
    Some(SnapPreview {
        zone,
        rect: snap_preview(work_area, zone),
    })
}

/// The preview currently shown, if any.
#[derive(Debug, Default)]
pub struct SnapState {
    preview: Option<SnapPreview>,
}

impl SnapState {
    /// The preview currently shown.
    pub fn preview(&self) -> Option<SnapPreview> {
        self.preview
    }

    /// Replaces the preview; returns true when it changed, so the caller can redraw.
    pub fn replace(&mut self, next: Option<SnapPreview>) -> bool {
        if self.preview == next {
            return false;
        }
        self.preview = next;
        true
    }
}

impl DesiliumState {
    /// The snap preview to draw, if a title bar drag currently hovers a zone.
    pub fn snap_preview(&self) -> Option<SnapPreview> {
        self.decoration.snap.preview()
    }

    /// Recomputes the preview for a window dragged with its pointer at `pointer`.
    /// Returns the zone under the pointer; the grab keeps it for the release.
    pub(crate) fn update_snap_preview(&mut self, id: WindowId, pointer: Point) -> Option<SnapZone> {
        let movable = self.wm.window_state(id) == Some(WindowState::Normal);
        let preview = preview_for_pointer(self.wm.work_area(), pointer, movable);
        if self.decoration.snap.replace(preview) {
            self.request_redraw();
        }
        preview.map(|preview| preview.zone)
    }

    /// Hides the preview (drag released or cancelled).
    pub(crate) fn clear_snap_preview(&mut self) {
        if self.decoration.snap.replace(None) {
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::new(0, 0, 1000, 800);

    fn at(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    #[test]
    fn no_preview_unless_dragging() {
        assert_eq!(preview_for_pointer(AREA, at(0, 400), false), None);
    }

    #[test]
    fn left_and_right_edges_snap_to_halves() {
        let left = preview_for_pointer(AREA, at(EDGE_BAND - 1, 400), true);
        assert_eq!(left.map(|p| p.zone), Some(SnapZone::Left));
        assert_eq!(left.map(|p| p.rect), Some(Rect::new(0, 0, 500, 800)));
        let right = preview_for_pointer(AREA, at(1000 - EDGE_BAND, 400), true);
        assert_eq!(right.map(|p| p.zone), Some(SnapZone::Right));
        assert_eq!(right.map(|p| p.rect), Some(Rect::new(500, 0, 500, 800)));
    }

    #[test]
    fn edge_band_is_exclusive() {
        assert_eq!(preview_for_pointer(AREA, at(EDGE_BAND, 400), true), None);
        assert_eq!(
            preview_for_pointer(AREA, at(1000 - EDGE_BAND - 1, 400), true),
            None
        );
    }

    #[test]
    fn top_edge_maximizes_to_the_work_area() {
        let preview = preview_for_pointer(AREA, at(500, 0), true);
        assert_eq!(preview.map(|p| p.zone), Some(SnapZone::Maximize));
        assert_eq!(preview.map(|p| p.rect), Some(AREA));
    }

    #[test]
    fn corners_use_the_corner_square_not_the_edge_band() {
        let inside_square = preview_for_pointer(AREA, at(CORNER_ZONE - 1, 20), true);
        assert_eq!(inside_square.map(|p| p.zone), Some(SnapZone::TopLeft));
        let beyond_square = preview_for_pointer(AREA, at(CORNER_ZONE, 4), true);
        assert_eq!(beyond_square.map(|p| p.zone), Some(SnapZone::Maximize));
    }

    #[test]
    fn corner_wins_over_edges_in_all_four_corners() {
        let cases = [
            (at(2, 2), SnapZone::TopLeft),
            (at(997, 2), SnapZone::TopRight),
            (at(2, 797), SnapZone::BottomLeft),
            (at(997, 797), SnapZone::BottomRight),
        ];
        for (pointer, zone) in cases {
            let preview = preview_for_pointer(AREA, pointer, true);
            assert_eq!(preview.map(|p| p.zone), Some(zone), "{pointer:?}");
        }
    }

    #[test]
    fn bottom_right_corner_rect_is_the_quarter() {
        let preview = preview_for_pointer(AREA, at(999, 799), true);
        assert_eq!(preview.map(|p| p.rect), Some(Rect::new(500, 400, 500, 400)));
    }

    #[test]
    fn pointer_near_top_but_far_from_sides_is_not_a_corner() {
        let preview = preview_for_pointer(AREA, at(500, CORNER_ZONE - 1), true);
        assert_eq!(preview, None);
    }

    #[test]
    fn pointer_outside_the_work_area_has_no_preview() {
        assert_eq!(preview_for_pointer(AREA, at(-1, 400), true), None);
        assert_eq!(preview_for_pointer(AREA, at(500, 800), true), None);
    }

    #[test]
    fn zones_follow_a_work_area_with_an_offset() {
        let area = Rect::new(0, 40, 1000, 760);
        let preview = preview_for_pointer(area, at(2, 41), true);
        assert_eq!(preview.map(|p| p.zone), Some(SnapZone::TopLeft));
        assert_eq!(preview.map(|p| p.rect), Some(Rect::new(0, 40, 500, 380)));
        assert_eq!(preview_for_pointer(area, at(500, 10), true), None);
    }

    #[test]
    fn state_reports_only_real_changes() {
        let mut state = SnapState::default();
        let preview = preview_for_pointer(AREA, at(0, 400), true);
        assert!(state.replace(preview));
        assert!(!state.replace(preview));
        assert_eq!(state.preview(), preview);
        assert!(state.replace(None));
        assert!(!state.replace(None));
        assert_eq!(state.preview(), None);
    }
}
