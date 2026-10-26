use crate::geometry::{Point, Rect, Size};

/// Which window edges an interactive resize drags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// Size bounds; a `max` component of 0 means unbounded and `min` is at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SizeLimits {
    pub min: Size,
    pub max: Size,
}

/// Which end of one axis moves during a resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisDrag {
    Start,
    End,
    Fixed,
}

/// Both edges set on one axis cancel out, so that axis stays put.
fn axis_drag(start_edge: bool, end_edge: bool) -> AxisDrag {
    match (start_edge, end_edge) {
        (true, false) => AxisDrag::Start,
        (false, true) => AxisDrag::End,
        _ => AxisDrag::Fixed,
    }
}

/// Size after dragging `edges` by the given deltas, clamped to `limits`.
pub fn resized_size(
    initial: Size,
    edges: Edges,
    delta_x: f64,
    delta_y: f64,
    limits: SizeLimits,
) -> Size {
    let width = resize_axis(
        initial.w,
        axis_delta(axis_drag(edges.left, edges.right), delta_x),
        limits.min.w,
        limits.max.w,
    );
    let height = resize_axis(
        initial.h,
        axis_delta(axis_drag(edges.top, edges.bottom), delta_y),
        limits.min.h,
        limits.max.h,
    );
    Size {
        w: width,
        h: height,
    }
}

/// Signed change in size: dragging the end edge grows, dragging the start edge shrinks.
fn axis_delta(drag: AxisDrag, delta: f64) -> f64 {
    match drag {
        AxisDrag::End => delta,
        AxisDrag::Start => -delta,
        AxisDrag::Fixed => 0.0,
    }
}

fn resize_axis(initial: i32, delta: f64, min: i32, max: i32) -> i32 {
    let lower = f64::from(min.max(1));
    let upper = if max > 0 {
        f64::from(max)
    } else {
        f64::INFINITY
    };
    let target = f64::from(initial) + delta;
    // `max`/`min` rather than `clamp` so a min above max cannot panic.
    target.max(lower).min(upper) as i32
}

/// Top-left location that keeps the edge opposite the dragged one fixed.
pub fn anchored_location(initial: Rect, edges: Edges, actual: Size) -> Point {
    Point {
        x: anchor_axis(
            initial.loc.x,
            initial.size.w,
            actual.w,
            axis_drag(edges.left, edges.right),
        ),
        y: anchor_axis(
            initial.loc.y,
            initial.size.h,
            actual.h,
            axis_drag(edges.top, edges.bottom),
        ),
    }
}

fn anchor_axis(origin: i32, initial_len: i32, actual_len: i32, drag: AxisDrag) -> i32 {
    match drag {
        AxisDrag::Start => origin
            .saturating_add(initial_len)
            .saturating_sub(actual_len),
        AxisDrag::End | AxisDrag::Fixed => origin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNBOUNDED: SizeLimits = SizeLimits {
        min: Size { w: 1, h: 1 },
        max: Size { w: 0, h: 0 },
    };

    fn edges(left: bool, right: bool, top: bool, bottom: bool) -> Edges {
        Edges {
            left,
            right,
            top,
            bottom,
        }
    }

    fn size(w: i32, h: i32) -> Size {
        Size { w, h }
    }

    #[test]
    fn right_edge_grows_with_positive_delta() {
        let out = resized_size(
            size(100, 50),
            edges(false, true, false, false),
            20.0,
            5.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(120, 50));
    }

    #[test]
    fn left_edge_grows_with_negative_delta() {
        let out = resized_size(
            size(100, 50),
            edges(true, false, false, false),
            -20.0,
            0.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(120, 50));
    }

    #[test]
    fn bottom_edge_changes_height() {
        let out = resized_size(
            size(100, 50),
            edges(false, false, false, true),
            99.0,
            10.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(100, 60));
    }

    #[test]
    fn top_edge_changes_height_inversely() {
        let out = resized_size(
            size(100, 50),
            edges(false, false, true, false),
            0.0,
            10.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(100, 40));
    }

    #[test]
    fn corner_changes_both_axes() {
        let out = resized_size(
            size(100, 50),
            edges(true, false, false, true),
            10.0,
            10.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(90, 60));
    }

    #[test]
    fn no_edges_keeps_size() {
        let out = resized_size(size(100, 50), Edges::default(), 10.0, 10.0, UNBOUNDED);
        assert_eq!(out, size(100, 50));
    }

    #[test]
    fn both_edges_on_an_axis_keep_that_axis() {
        let out = resized_size(
            size(100, 50),
            edges(true, true, true, true),
            10.0,
            10.0,
            UNBOUNDED,
        );
        assert_eq!(out, size(100, 50));
    }

    #[test]
    fn clamps_to_min_and_max() {
        let limits = SizeLimits {
            min: size(80, 40),
            max: size(200, 100),
        };
        let shrunk = resized_size(
            size(100, 50),
            edges(false, true, false, true),
            -500.0,
            -500.0,
            limits,
        );
        let grown = resized_size(
            size(100, 50),
            edges(false, true, false, true),
            500.0,
            500.0,
            limits,
        );
        assert_eq!(shrunk, size(80, 40));
        assert_eq!(grown, size(200, 100));
    }

    #[test]
    fn min_below_one_is_treated_as_one() {
        let limits = SizeLimits {
            min: size(0, -5),
            max: Size::default(),
        };
        let out = resized_size(
            size(10, 10),
            edges(false, true, false, true),
            -100.0,
            -100.0,
            limits,
        );
        assert_eq!(out, size(1, 1));
    }

    #[test]
    fn zero_max_is_unbounded() {
        let out = resized_size(
            size(10, 10),
            edges(false, true, false, false),
            1_000_000.0,
            0.0,
            UNBOUNDED,
        );
        assert_eq!(out.w, 1_000_010);
    }

    #[test]
    fn min_above_max_does_not_panic() {
        let limits = SizeLimits {
            min: size(300, 300),
            max: size(200, 200),
        };
        let out = resized_size(
            size(100, 100),
            edges(false, true, false, true),
            0.0,
            0.0,
            limits,
        );
        assert_eq!(out, size(200, 200));
    }

    #[test]
    fn anchored_left_edge_keeps_right_edge_fixed() {
        let initial = Rect::new(100, 50, 200, 100);
        let loc = anchored_location(initial, edges(true, false, false, false), size(150, 100));
        assert_eq!(loc, Point { x: 150, y: 50 });
    }

    #[test]
    fn anchored_top_edge_keeps_bottom_edge_fixed() {
        let initial = Rect::new(100, 50, 200, 100);
        let loc = anchored_location(initial, edges(false, false, true, false), size(200, 70));
        assert_eq!(loc, Point { x: 100, y: 80 });
    }

    #[test]
    fn anchored_right_edge_keeps_origin() {
        let initial = Rect::new(100, 50, 200, 100);
        let loc = anchored_location(initial, edges(false, true, false, true), size(300, 300));
        assert_eq!(loc, Point { x: 100, y: 50 });
    }
}
