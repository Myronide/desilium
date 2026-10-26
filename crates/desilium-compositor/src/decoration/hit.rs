//! What a pointer position hits on a window frame.

use super::geometry::{Button, Frame};
use super::metrics::{CORNER_ZONE, Chrome, RESIZE_INSIDE, RESIZE_OUTSIDE};
use desilium_wm::{Edges, Point, Rect};

/// Result of hit testing one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// The client surface; the client gets the pointer.
    Client,
    /// Draggable title bar background.
    TitleBar,
    /// A window button.
    Button(Button),
    /// Resize zone dragging these edges.
    ResizeBorder(Edges),
}

impl Hit {
    /// True when the compositor, not the client, owns the pointer here.
    pub fn is_chrome(self) -> bool {
        self != Hit::Client
    }
}

/// Tests `point` against `frame`; `None` means the frame does not cover the point at all.
pub fn hit_test(frame: &Frame, point: Point) -> Option<Hit> {
    if frame.chrome == Chrome::None {
        return frame.outer.contains(point).then_some(Hit::Client);
    }
    if let Some(edges) = resize_edges(frame, point) {
        return Some(Hit::ResizeBorder(edges));
    }
    if !frame.outer.contains(point) {
        return None;
    }
    if let Some(button) = Button::ALL
        .into_iter()
        .find(|button| frame.button_rect(*button).contains(point))
    {
        return Some(Hit::Button(button));
    }
    if frame.title_bar.contains(point) {
        return Some(Hit::TitleBar);
    }
    Some(Hit::Client)
}

fn resize_edges(frame: &Frame, point: Point) -> Option<Edges> {
    if frame.chrome != Chrome::Windowed {
        return None;
    }
    let outer = frame.outer;
    let reach = Rect::new(
        outer.loc.x - RESIZE_OUTSIDE,
        outer.loc.y - RESIZE_OUTSIDE,
        outer.size.w + RESIZE_OUTSIDE * 2,
        outer.size.h + RESIZE_OUTSIDE * 2,
    );
    if !reach.contains(point) {
        return None;
    }
    let mut edges = Edges {
        left: point.x < outer.loc.x + RESIZE_INSIDE,
        right: point.x >= outer.right() - RESIZE_INSIDE,
        top: point.y < outer.loc.y + RESIZE_INSIDE,
        bottom: point.y >= outer.bottom() - RESIZE_INSIDE,
    };
    let on_vertical_band = edges.left || edges.right;
    let on_horizontal_band = edges.top || edges.bottom;
    if !on_vertical_band && !on_horizontal_band {
        return None;
    }
    if on_vertical_band && !on_horizontal_band {
        edges.top = point.y < outer.loc.y + CORNER_ZONE;
        edges.bottom = point.y >= outer.bottom() - CORNER_ZONE;
    }
    if on_horizontal_band && !on_vertical_band {
        edges.left = point.x < outer.loc.x + CORNER_ZONE;
        edges.right = point.x >= outer.right() - CORNER_ZONE;
    }
    Some(edges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_theme::Theme;

    const OUTER: Rect = Rect::new(100, 100, 400, 300);

    fn metrics() -> crate::decoration::Metrics {
        crate::decoration::Metrics::from_theme(&Theme::dark())
    }

    fn hit(chrome: Chrome, x: i32, y: i32) -> Option<Hit> {
        let metrics = metrics();
        let frame = Frame::new(&metrics, chrome, OUTER);
        hit_test(&frame, Point { x, y })
    }

    fn edges(left: bool, right: bool, top: bool, bottom: bool) -> Option<Hit> {
        Some(Hit::ResizeBorder(Edges {
            left,
            right,
            top,
            bottom,
        }))
    }

    #[test]
    fn client_title_and_buttons() {
        assert_eq!(hit(Chrome::Windowed, 300, 300), Some(Hit::Client));
        assert_eq!(hit(Chrome::Windowed, 200, 118), Some(Hit::TitleBar));
        assert_eq!(
            hit(Chrome::Windowed, 100 + 364 + 5, 110),
            Some(Hit::Button(Button::Close))
        );
        assert_eq!(
            hit(Chrome::Windowed, 100 + 300, 110),
            Some(Hit::Button(Button::Minimize))
        );
        assert_eq!(
            hit(Chrome::Windowed, 100 + 345, 110),
            Some(Hit::Button(Button::Maximize))
        );
    }

    #[test]
    fn gap_between_buttons_is_title_bar() {
        assert_eq!(hit(Chrome::Windowed, 100 + 328, 110), Some(Hit::TitleBar));
    }

    #[test]
    fn side_edges_resize_horizontally() {
        assert_eq!(
            hit(Chrome::Windowed, 100, 250),
            edges(true, false, false, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 499, 250),
            edges(false, true, false, false)
        );
    }

    #[test]
    fn zone_extends_outside_the_frame() {
        assert_eq!(
            hit(Chrome::Windowed, 96, 250),
            edges(true, false, false, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 503, 250),
            edges(false, true, false, false)
        );
        assert_eq!(hit(Chrome::Windowed, 95, 250), None);
        assert_eq!(hit(Chrome::Windowed, 504, 250), None);
    }

    #[test]
    fn top_and_bottom_edges() {
        assert_eq!(
            hit(Chrome::Windowed, 300, 97),
            edges(false, false, true, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 300, 399),
            edges(false, false, false, true)
        );
    }

    #[test]
    fn corners_are_generous() {
        assert_eq!(
            hit(Chrome::Windowed, 100, 100),
            edges(true, false, true, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 100, 110),
            edges(true, false, true, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 110, 100),
            edges(true, false, true, false)
        );
        assert_eq!(
            hit(Chrome::Windowed, 499, 399),
            edges(false, true, false, true)
        );
        assert_eq!(
            hit(Chrome::Windowed, 98, 399),
            edges(true, false, false, true)
        );
        assert_eq!(
            hit(Chrome::Windowed, 499, 100),
            edges(false, true, true, false)
        );
    }

    #[test]
    fn mid_edge_is_not_a_corner() {
        assert_eq!(
            hit(Chrome::Windowed, 100, 130),
            edges(true, false, false, false)
        );
    }

    #[test]
    fn buttons_stay_clickable_next_to_the_edge() {
        assert_eq!(hit(Chrome::Windowed, 100 + 396, 110), Some(Hit::TitleBar));
        assert_eq!(
            hit(Chrome::Windowed, 100 + 395, 110),
            Some(Hit::Button(Button::Close))
        );
    }

    #[test]
    fn maximized_has_no_resize_zones() {
        assert_eq!(hit(Chrome::Maximized, 100, 250), Some(Hit::Client));
        assert_eq!(hit(Chrome::Maximized, 96, 250), None);
        assert_eq!(hit(Chrome::Maximized, 300, 100), Some(Hit::TitleBar));
    }

    #[test]
    fn undecorated_windows_only_have_a_client() {
        assert_eq!(hit(Chrome::None, 100, 100), Some(Hit::Client));
        assert_eq!(hit(Chrome::None, 99, 100), None);
    }

    #[test]
    fn outside_the_frame_misses() {
        assert_eq!(hit(Chrome::Windowed, 0, 0), None);
    }

    #[test]
    fn chrome_flag() {
        assert!(!Hit::Client.is_chrome());
        assert!(Hit::TitleBar.is_chrome());
    }
}
