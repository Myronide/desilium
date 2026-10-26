//! Pure geometry and sizing math of the backdrop blur (no GL, fully unit-testable).

use smithay::utils::{Physical, Rectangle};

/// Deepest down-sample level; caps the blur at `MAX_LEVEL_SIGMA * 2^6` physical px.
pub(super) const MAX_DOWNSAMPLES: u32 = 6;
/// Gaussian sigma per theme radius px: the theme radius is the full visible blur extent
/// (about two sigma), like the CSS `box-shadow` blur value.
const SIGMA_PER_RADIUS: f32 = 0.5;
/// Largest sigma (texels) the separable Gaussian handles at the lowest level; 8 merged
/// tap pairs reach 16 texels, i.e. more than four sigma.
const MAX_LEVEL_SIGMA: f32 = 3.5;
/// Below this the blur is invisible; avoids a degenerate kernel.
const MIN_SIGMA: f32 = 0.35;
/// How many sigma of the real backdrop are grabbed around the glass rectangle.
const MARGIN_SIGMAS: f32 = 3.0;
/// Texture capacities are rounded up to this, so animated resizes rarely reallocate.
const CAPACITY_ALIGN: i32 = 64;

/// Integer pixel rectangle in framebuffer space (origin bottom-left, like GL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Grows by `margin` on every side.
    pub fn expand(&self, margin: i32) -> PxRect {
        PxRect {
            x: self.x.saturating_sub(margin),
            y: self.y.saturating_sub(margin),
            w: self.w.saturating_add(margin.saturating_mul(2)),
            h: self.h.saturating_add(margin.saturating_mul(2)),
        }
    }

    /// Intersection with `[0, width) x [0, height)`; may be empty.
    pub fn clamp_to(&self, width: i32, height: i32) -> PxRect {
        let x0 = self.x.clamp(0, width);
        let y0 = self.y.clamp(0, height);
        let x1 = self.x.saturating_add(self.w).clamp(0, width);
        let y1 = self.y.saturating_add(self.h).clamp(0, height);
        PxRect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }
}

/// Pass structure derived from the blur radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BlurPlan {
    /// Total Gaussian sigma in physical px.
    pub sigma_px: f32,
    /// Number of 2x down-sample passes before the Gaussian.
    pub downsamples: u32,
    /// Gaussian sigma in texels of the lowest level.
    pub level_sigma: f32,
    /// Extra backdrop (physical px) grabbed around the glass rectangle.
    pub margin: i32,
}

impl BlurPlan {
    /// Plans a blur of `radius` logical px at output `scale`.
    pub fn new(radius: f32, scale: f64) -> BlurPlan {
        let radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };
        let scale = if scale.is_finite() {
            scale.max(0.1) as f32
        } else {
            1.0
        };
        let sigma_px = (radius * scale * SIGMA_PER_RADIUS).max(MIN_SIGMA);
        let mut downsamples = 0;
        let mut level_sigma = sigma_px;
        while level_sigma > MAX_LEVEL_SIGMA && downsamples < MAX_DOWNSAMPLES {
            downsamples += 1;
            level_sigma *= 0.5;
        }
        BlurPlan {
            sigma_px,
            downsamples,
            level_sigma: level_sigma.min(MAX_LEVEL_SIGMA * 1.5),
            margin: (sigma_px * MARGIN_SIGMAS).ceil() as i32,
        }
    }
}

/// Size of down-sample `level` for a level-0 size, rounding up so no content is lost.
pub(super) fn level_size(size: (i32, i32), level: u32) -> (i32, i32) {
    let divisor = 1_i32 << level.min(MAX_DOWNSAMPLES);
    let shrink = |value: i32| (value.saturating_add(divisor - 1) / divisor).max(1);
    (shrink(size.0), shrink(size.1))
}

/// Rounds a texture capacity up to the allocation granularity.
pub(super) fn align_capacity(size: (i32, i32)) -> (i32, i32) {
    let align = |value: i32| {
        value
            .max(1)
            .saturating_add(CAPACITY_ALIGN - 1)
            .div_euclid(CAPACITY_ALIGN)
            .saturating_mul(CAPACITY_ALIGN)
    };
    (align(size.0), align(size.1))
}

/// Applies the column-major 3x3 projection of a smithay frame to an output-space point and
/// returns clip-space coordinates.
pub(super) fn to_clip(projection: &[f32; 9], x: f32, y: f32) -> (f32, f32) {
    (
        projection[0] * x + projection[3] * y + projection[6],
        projection[1] * x + projection[4] * y + projection[7],
    )
}

fn to_framebuffer(projection: &[f32; 9], point: (f32, f32), fb: (i32, i32)) -> (f32, f32) {
    let (clip_x, clip_y) = to_clip(projection, point.0, point.1);
    (
        (clip_x * 0.5 + 0.5) * fb.0 as f32,
        (clip_y * 0.5 + 0.5) * fb.1 as f32,
    )
}

fn corners_of(rect: Rectangle<i32, Physical>) -> [(f32, f32); 4] {
    let (left, top) = (rect.loc.x as f32, rect.loc.y as f32);
    let (right, bottom) = (left + rect.size.w as f32, top + rect.size.h as f32);
    // Order matches `Corners`: top-left, top-right, bottom-right, bottom-left.
    [(left, top), (right, top), (right, bottom), (left, bottom)]
}

/// Maps an output-space rectangle to framebuffer pixels (handles any frame transform).
pub(super) fn framebuffer_rect(
    projection: &[f32; 9],
    rect: Rectangle<i32, Physical>,
    fb: (i32, i32),
) -> PxRect {
    let mapped = corners_of(rect).map(|corner| to_framebuffer(projection, corner, fb));
    let fold = |pick: fn((f32, f32)) -> f32, init: f32, merge: fn(f32, f32) -> f32| {
        mapped
            .iter()
            .fold(init, |acc, point| merge(acc, pick(*point)))
    };
    let left = fold(|p| p.0, f32::MAX, f32::min).round() as i32;
    let right = fold(|p| p.0, f32::MIN, f32::max).round() as i32;
    let bottom = fold(|p| p.1, f32::MAX, f32::min).round() as i32;
    let top = fold(|p| p.1, f32::MIN, f32::max).round() as i32;
    PxRect {
        x: left,
        y: bottom,
        w: right - left,
        h: top - bottom,
    }
}

/// Reorders `[top-left, top-right, bottom-right, bottom-left]` radii (output space) into the
/// shader's framebuffer order `[bottom-left, bottom-right, top-right, top-left]` (y up),
/// following where the frame transform actually puts each corner.
pub(super) fn framebuffer_radii(
    projection: &[f32; 9],
    rect: Rectangle<i32, Physical>,
    fb: (i32, i32),
    radii: [f32; 4],
) -> [f32; 4] {
    let bounds = framebuffer_rect(projection, rect, fb);
    let center = (
        bounds.x as f32 + bounds.w as f32 * 0.5,
        bounds.y as f32 + bounds.h as f32 * 0.5,
    );
    let mut out = [0.0; 4];
    for (corner, radius) in corners_of(rect).into_iter().zip(radii) {
        let (px, py) = to_framebuffer(projection, corner, fb);
        let slot = match (px > center.0, py > center.1) {
            (false, false) => 0,
            (true, false) => 1,
            (true, true) => 2,
            (false, true) => 3,
        };
        out[slot] = radius;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Projection of `GlesFrame::render` for `Transform::Normal` on a `w x h` target.
    fn normal(w: f32, h: f32) -> [f32; 9] {
        [2.0 / w, 0.0, 0.0, 0.0, 2.0 / h, 0.0, -1.0, -1.0, 1.0]
    }

    /// Same for `Transform::Flipped180`: output y runs top-down in framebuffer space.
    fn flipped(w: f32, h: f32) -> [f32; 9] {
        [2.0 / w, 0.0, 0.0, 0.0, -2.0 / h, 0.0, -1.0, 1.0, 1.0]
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn plan_keeps_level_sigma_bounded_up_to_large_radii() {
        for radius in [0.0, 1.0, 8.0, 24.0, 48.0, 96.0, 400.0] {
            for scale in [1.0, 1.5, 2.0, 3.0] {
                let plan = BlurPlan::new(radius, scale);
                assert!(plan.level_sigma <= MAX_LEVEL_SIGMA * 1.5, "{plan:?}");
                assert!(plan.downsamples <= MAX_DOWNSAMPLES);
            }
        }
    }

    #[test]
    fn plan_scales_sigma_with_output_scale() {
        let one = BlurPlan::new(24.0, 1.0);
        let two = BlurPlan::new(24.0, 2.0);
        assert!((two.sigma_px - one.sigma_px * 2.0).abs() < 1e-4);
        assert_eq!(two.downsamples, one.downsamples + 1);
    }

    #[test]
    fn plan_survives_garbage_input() {
        let plan = BlurPlan::new(f32::NAN, f64::INFINITY);
        assert!(plan.sigma_px.is_finite() && plan.sigma_px > 0.0);
        assert_eq!(plan.downsamples, 0);
    }

    #[test]
    fn level_sizes_round_up() {
        assert_eq!(level_size((101, 33), 0), (101, 33));
        assert_eq!(level_size((101, 33), 1), (51, 17));
        assert_eq!(level_size((101, 33), 3), (13, 5));
        assert_eq!(level_size((1, 1), 6), (1, 1));
    }

    #[test]
    fn capacity_aligns_up() {
        assert_eq!(align_capacity((1, 64)), (64, 64));
        assert_eq!(align_capacity((65, 130)), (128, 192));
    }

    #[test]
    fn rect_clamping_handles_offscreen_and_empty() {
        let r = PxRect {
            x: -10,
            y: 5,
            w: 50,
            h: 20,
        };
        assert_eq!(
            r.clamp_to(100, 100),
            PxRect {
                x: 0,
                y: 5,
                w: 40,
                h: 20
            }
        );
        let away = PxRect {
            x: 300,
            y: 0,
            w: 10,
            h: 10,
        };
        assert!(away.clamp_to(100, 100).is_empty());
    }

    #[test]
    fn normal_transform_puts_output_top_at_framebuffer_bottom() {
        let mapped = framebuffer_rect(&normal(200.0, 100.0), rect(10, 0, 40, 30), (200, 100));
        assert_eq!(
            mapped,
            PxRect {
                x: 10,
                y: 0,
                w: 40,
                h: 30
            }
        );
    }

    #[test]
    fn flipped_transform_maps_output_top_to_framebuffer_top() {
        let mapped = framebuffer_rect(&flipped(200.0, 100.0), rect(10, 0, 40, 30), (200, 100));
        assert_eq!(
            mapped,
            PxRect {
                x: 10,
                y: 70,
                w: 40,
                h: 30
            }
        );
    }

    #[test]
    fn radii_follow_the_corners_through_flips() {
        let radii = [1.0, 2.0, 3.0, 4.0]; // tl, tr, br, bl
        let area = rect(10, 10, 40, 30);
        let normal = framebuffer_radii(&normal(200.0, 100.0), area, (200, 100), radii);
        assert_eq!(normal, [1.0, 2.0, 3.0, 4.0]);
        let flipped = framebuffer_radii(&flipped(200.0, 100.0), area, (200, 100), radii);
        assert_eq!(flipped, [4.0, 3.0, 2.0, 1.0]);
    }
}
