/// A logical position in compositor coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A logical width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

/// An axis-aligned rectangle given by its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub loc: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            loc: Point { x, y },
            size: Size { w, h },
        }
    }

    /// Left and top are inclusive, right and bottom are exclusive.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.loc.x
            && point.x < self.right()
            && point.y >= self.loc.y
            && point.y < self.bottom()
    }

    pub fn right(&self) -> i32 {
        self.loc.x.saturating_add(self.size.w)
    }

    pub fn bottom(&self) -> i32 {
        self.loc.y.saturating_add(self.size.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_includes_top_left_edge() {
        let rect = Rect::new(10, 20, 30, 40);
        assert!(rect.contains(Point { x: 10, y: 20 }));
    }

    #[test]
    fn contains_excludes_right_and_bottom_edges() {
        let rect = Rect::new(10, 20, 30, 40);
        assert!(!rect.contains(Point { x: 40, y: 20 }));
        assert!(!rect.contains(Point { x: 10, y: 60 }));
        assert!(rect.contains(Point { x: 39, y: 59 }));
    }

    #[test]
    fn contains_rejects_points_outside() {
        let rect = Rect::new(10, 20, 30, 40);
        assert!(!rect.contains(Point { x: 9, y: 20 }));
        assert!(!rect.contains(Point { x: 10, y: 19 }));
    }

    #[test]
    fn empty_rect_contains_nothing() {
        let rect = Rect::new(5, 5, 0, 0);
        assert!(!rect.contains(Point { x: 5, y: 5 }));
    }

    #[test]
    fn right_and_bottom_saturate_instead_of_overflowing() {
        let rect = Rect::new(i32::MAX, i32::MAX, 10, 10);
        assert_eq!(rect.right(), i32::MAX);
        assert_eq!(rect.bottom(), i32::MAX);
    }
}
