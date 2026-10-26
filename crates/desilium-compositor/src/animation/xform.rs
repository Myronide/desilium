//! Axis-aligned 2D transform (scale + translation) plus opacity, applied to whole windows.
//!
//! Every animated pose of a window is expressed as one of these: the window is laid out at its
//! final geometry, and the transform maps that layout to what is on screen. Rectangles are
//! snapped edge by edge so neighbouring pieces (border, title bar, client) keep tiling.

use desilium_wm::{Point, Rect, Size};

/// Rectangle in logical pixels with fractional coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    /// Exact copy of an integer rectangle.
    pub fn from_rect(rect: Rect) -> RectF {
        RectF {
            x: rect.loc.x as f32,
            y: rect.loc.y as f32,
            w: rect.size.w as f32,
            h: rect.size.h as f32,
        }
    }

    /// Nearest integer rectangle (at least 1x1).
    pub fn to_rect(self) -> Rect {
        Rect::new(
            self.x.round() as i32,
            self.y.round() as i32,
            (self.w.round() as i32).max(1),
            (self.h.round() as i32).max(1),
        )
    }

    /// As a tween-friendly array `[x, y, w, h]`.
    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    /// Inverse of [`RectF::to_array`].
    pub fn from_array(values: [f32; 4]) -> RectF {
        RectF {
            x: values[0],
            y: values[1],
            w: values[2],
            h: values[3],
        }
    }

    /// Centre point.
    pub fn centre(self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Scale and translation (`p' = s * p + t` per axis) with an opacity multiplier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xform {
    pub sx: f64,
    pub sy: f64,
    pub tx: f64,
    pub ty: f64,
    /// Opacity multiplier for everything the window draws.
    pub alpha: f32,
}

impl Xform {
    /// Leaves everything as it is.
    pub const IDENTITY: Xform = Xform {
        sx: 1.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
        alpha: 1.0,
    };

    /// Maps `from` onto `to`; degenerate sources only translate.
    pub fn mapping(from: RectF, to: RectF) -> Xform {
        let ratio = |source: f32, target: f32| {
            if source > 0.0 && target >= 0.0 {
                f64::from(target) / f64::from(source)
            } else {
                1.0
            }
        };
        let sx = ratio(from.w, to.w);
        let sy = ratio(from.h, to.h);
        Xform {
            sx,
            sy,
            tx: f64::from(to.x) - f64::from(from.x) * sx,
            ty: f64::from(to.y) - f64::from(from.y) * sy,
            alpha: 1.0,
        }
    }

    /// Uniform scale `scale` around the point `centre`.
    pub fn scale_about(centre: (f32, f32), scale: f32) -> Xform {
        let scale = f64::from(scale);
        Xform {
            sx: scale,
            sy: scale,
            tx: (1.0 - scale) * f64::from(centre.0),
            ty: (1.0 - scale) * f64::from(centre.1),
            alpha: 1.0,
        }
    }

    /// Applies `self` first, then `next`; opacities multiply.
    pub fn then(self, next: Xform) -> Xform {
        Xform {
            sx: self.sx * next.sx,
            sy: self.sy * next.sy,
            tx: next.sx * self.tx + next.tx,
            ty: next.sy * self.ty + next.ty,
            alpha: self.alpha * next.alpha,
        }
    }

    /// Same transform with opacity multiplied by `alpha` (clamped to `0..=1`).
    pub fn with_alpha(mut self, alpha: f32) -> Xform {
        self.alpha *= alpha.clamp(0.0, 1.0);
        self
    }

    /// True when applying the transform changes nothing.
    pub fn is_identity(&self) -> bool {
        *self == Xform::IDENTITY
    }

    /// Mean scale, used for radii, blur and border widths.
    pub fn uniform(&self) -> f32 {
        (self.sx * self.sy).abs().sqrt() as f32
    }

    /// Maps a point.
    pub fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.sx * x + self.tx, self.sy * y + self.ty)
    }

    /// Maps a rectangle, rounding each edge so that touching rectangles still touch.
    pub fn rect(&self, rect: Rect) -> Rect {
        if rect.size.w <= 0 || rect.size.h <= 0 {
            return rect;
        }
        let (left, top) = self.point(f64::from(rect.loc.x), f64::from(rect.loc.y));
        let right_edge = f64::from(rect.loc.x.saturating_add(rect.size.w));
        let bottom_edge = f64::from(rect.loc.y.saturating_add(rect.size.h));
        let (right, bottom) = self.point(right_edge, bottom_edge);
        let (left, top) = (left.round() as i32, top.round() as i32);
        Rect {
            loc: Point { x: left, y: top },
            size: Size {
                w: (right.round() as i32).saturating_sub(left).max(1),
                h: (bottom.round() as i32).saturating_sub(top).max(1),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn identity_changes_nothing() {
        let source = rect(13, -7, 801, 433);
        assert!(Xform::IDENTITY.is_identity());
        assert_eq!(Xform::IDENTITY.rect(source), source);
    }

    #[test]
    fn mapping_onto_itself_is_exactly_identity() {
        let source = RectF::from_rect(rect(13, 29, 801, 433));
        assert!(Xform::mapping(source, source).is_identity());
        assert!(Xform::scale_about(source.centre(), 1.0).is_identity());
    }

    #[test]
    fn scale_about_keeps_the_centre_fixed() {
        let source = rect(100, 100, 400, 200);
        let xf = Xform::scale_about((300.0, 200.0), 0.5);
        assert_eq!(xf.rect(source), rect(200, 150, 200, 100));
    }

    #[test]
    fn mapping_moves_and_resizes() {
        let from = RectF::from_rect(rect(0, 0, 100, 100));
        let to = RectF::from_rect(rect(50, 20, 200, 50));
        let xf = Xform::mapping(from, to);
        assert_eq!(xf.rect(rect(0, 0, 100, 100)), rect(50, 20, 200, 50));
        assert_eq!(xf.rect(rect(50, 50, 50, 50)), rect(150, 45, 100, 25));
    }

    #[test]
    fn then_applies_in_order() {
        let first = Xform::mapping(
            RectF::from_rect(rect(0, 0, 10, 10)),
            RectF::from_rect(rect(10, 0, 10, 10)),
        );
        let second = Xform::scale_about((0.0, 0.0), 2.0);
        let both = first.then(second);
        assert_eq!(both.point(0.0, 0.0), (20.0, 0.0));
        assert_eq!(both.point(1.0, 1.0), (22.0, 2.0));
    }

    #[test]
    fn opacity_multiplies_and_clamps() {
        let xf = Xform::IDENTITY.with_alpha(0.5).with_alpha(0.5);
        assert_eq!(xf.alpha, 0.25);
        assert_eq!(Xform::IDENTITY.with_alpha(7.0).alpha, 1.0);
        assert_eq!(Xform::IDENTITY.with_alpha(-1.0).alpha, 0.0);
    }

    #[test]
    fn adjacent_rects_still_touch_after_rounding() {
        let xf = Xform::scale_about((50.0, 50.0), 0.937);
        let left = xf.rect(rect(0, 0, 33, 40));
        let right = xf.rect(rect(33, 0, 67, 40));
        assert_eq!(left.loc.x + left.size.w, right.loc.x);
    }

    #[test]
    fn degenerate_rects_pass_through_and_never_vanish() {
        let xf = Xform::scale_about((0.0, 0.0), 0.01);
        assert_eq!(xf.rect(rect(5, 5, 0, 0)), rect(5, 5, 0, 0));
        assert!(xf.rect(rect(5, 5, 10, 10)).size.w >= 1);
    }

    #[test]
    fn mapping_from_an_empty_source_only_translates() {
        let xf = Xform::mapping(
            RectF::from_rect(rect(0, 0, 0, 0)),
            RectF::from_rect(rect(9, 9, 5, 5)),
        );
        assert_eq!((xf.sx, xf.sy), (1.0, 1.0));
    }
}
