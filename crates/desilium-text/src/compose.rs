//! Pixel composition: coverage gamma, premultiplied color math and masks blitted onto a canvas.

/// Exponent applied to glyph coverage. Values below 1 thicken anti-aliased edges, so light text
/// on dark backgrounds does not look thinner than dark text on light ones. Tuned by eye.
pub const COVERAGE_GAMMA: f32 = 0.8;

const BYTE_MAX: u8 = u8::MAX;
const ROUNDING_BIAS: u32 = 128;
const BYTE_SHIFT: u32 = 8;
const CHANNELS: usize = 4;

/// Multiplies two 8-bit values interpreted as fractions of 255, rounded to nearest.
pub fn mul_u8(value: u8, factor: u8) -> u8 {
    let product = u32::from(value) * u32::from(factor) + ROUNDING_BIAS;
    ((product + (product >> BYTE_SHIFT)) >> BYTE_SHIFT) as u8
}

/// Builds the coverage lookup table that applies [`COVERAGE_GAMMA`].
pub fn coverage_lut() -> [u8; 256] {
    let mut table = [0u8; 256];
    for (index, slot) in table.iter_mut().enumerate() {
        let linear = index as f32 / f32::from(BYTE_MAX);
        *slot = (linear.powf(COVERAGE_GAMMA) * f32::from(BYTE_MAX)).round() as u8;
    }
    table
}

/// Looks up a coverage value in a table from [`coverage_lut`].
pub fn apply_lut(lut: &[u8; 256], coverage: u8) -> u8 {
    lut.get(usize::from(coverage)).copied().unwrap_or(coverage)
}

/// Premultiplies a straight RGBA color by a coverage value.
pub fn premultiply(color: [u8; 4], coverage: u8) -> [u8; 4] {
    let [red, green, blue, alpha] = color;
    let alpha = mul_u8(alpha, coverage);
    [
        mul_u8(red, alpha),
        mul_u8(green, alpha),
        mul_u8(blue, alpha),
        alpha,
    ]
}

/// Porter-Duff "over" for premultiplied RGBA: `dst = src + dst * (1 - src_alpha)`.
pub fn blend_over(dst: &mut [u8], src: [u8; 4]) {
    let [_, _, _, source_alpha] = src;
    let remaining = BYTE_MAX - source_alpha;
    for (channel, source) in dst.iter_mut().zip(src) {
        *channel = source.saturating_add(mul_u8(*channel, remaining));
    }
}

/// A coverage mask: one byte per pixel, row-major.
pub struct Mask<'a> {
    /// Mask width in pixels.
    pub width: usize,
    /// Mask height in pixels.
    pub height: usize,
    /// `width * height` coverage values.
    pub coverage: &'a [u8],
}

/// Premultiplied RGBA pixel buffer that masks are composited into.
pub struct Canvas {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major premultiplied RGBA, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl Canvas {
    /// Creates a transparent canvas.
    pub fn new(width: u32, height: u32) -> Self {
        let pixel_count = usize::try_from(width)
            .unwrap_or(0)
            .saturating_mul(usize::try_from(height).unwrap_or(0));
        Self {
            width,
            height,
            pixels: vec![0; pixel_count.saturating_mul(CHANNELS)],
        }
    }

    /// Composites `mask` with its top-left corner at `origin`, tinted by `color`, clipped to the canvas.
    pub fn blit(&mut self, mask: &Mask<'_>, origin: (i32, i32), color: [u8; 4]) {
        for row in 0..mask.height {
            if let Some(y) = clip_index(origin.1, row, self.height) {
                self.blit_row(mask, row, origin.0, y, color);
            }
        }
    }

    fn blit_row(&mut self, mask: &Mask<'_>, row: usize, origin_x: i32, y: usize, color: [u8; 4]) {
        let row_start = row.saturating_mul(mask.width);
        for col in 0..mask.width {
            let Some(x) = clip_index(origin_x, col, self.width) else {
                continue;
            };
            let coverage = mask
                .coverage
                .get(row_start.saturating_add(col))
                .copied()
                .unwrap_or(0);
            if coverage == 0 {
                continue;
            }
            let index = y
                .saturating_mul(self.width as usize)
                .saturating_add(x)
                .saturating_mul(CHANNELS);
            if let Some(dst) = self.pixels.get_mut(index..index.saturating_add(CHANNELS)) {
                blend_over(dst, premultiply(color, coverage));
            }
        }
    }
}

/// Maps `origin + offset` to a canvas index in `0..limit`, or None when it falls outside.
fn clip_index(origin: i32, offset: usize, limit: u32) -> Option<usize> {
    let position = i64::from(origin).checked_add(i64::try_from(offset).ok()?)?;
    let inside = usize::try_from(position).ok()?;
    (inside < usize::try_from(limit).ok()?).then_some(inside)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_u8_is_exact_at_endpoints_and_rounds() {
        assert_eq!(mul_u8(255, 255), 255);
        assert_eq!(mul_u8(0, 255), 0);
        assert_eq!(mul_u8(255, 0), 0);
        assert_eq!(mul_u8(128, 255), 128);
        assert_eq!(mul_u8(100, 128), 50);
    }

    #[test]
    fn coverage_gamma_keeps_endpoints_and_thickens_midtones() {
        let lut = coverage_lut();
        assert_eq!(apply_lut(&lut, 0), 0);
        assert_eq!(apply_lut(&lut, 255), 255);
        assert!(
            apply_lut(&lut, 128) > 128,
            "gamma below 1 must boost midtones"
        );
        let monotonic = (1..=255u8).all(|c| apply_lut(&lut, c) >= apply_lut(&lut, c - 1));
        assert!(monotonic);
    }

    #[test]
    fn premultiplied_output_never_exceeds_alpha() {
        for red in (0..=255u8).step_by(15) {
            for green in (0..=255u8).step_by(15) {
                for blue in (0..=255u8).step_by(15) {
                    for coverage in (0..=255u8).step_by(15) {
                        let [r, g, b, a] = premultiply([red, green, blue, 200], coverage);
                        assert!(r <= a && g <= a && b <= a);
                    }
                }
            }
        }
    }

    #[test]
    fn blend_over_preserves_premultiplied_invariant() {
        for src_alpha in (0..=255u8).step_by(17) {
            for dst_alpha in (0..=255u8).step_by(17) {
                let src = [src_alpha / 2, src_alpha / 3, src_alpha, src_alpha];
                let mut dst = [dst_alpha / 2, dst_alpha / 4, dst_alpha, dst_alpha];
                blend_over(&mut dst, src);
                let [r, g, b, a] = dst;
                assert!(r <= a && g <= a && b <= a, "{dst:?}");
            }
        }
    }

    #[test]
    fn opaque_source_replaces_destination() {
        let mut dst = [10, 20, 30, 40];
        blend_over(&mut dst, [255, 255, 255, 255]);
        assert_eq!(dst, [255, 255, 255, 255]);
    }

    #[test]
    fn blit_clips_masks_partially_outside_the_canvas() {
        let mut canvas = Canvas::new(2, 2);
        let coverage = [255u8; 9];
        let mask = Mask {
            width: 3,
            height: 3,
            coverage: &coverage,
        };
        canvas.blit(&mask, (-1, -1), [255, 255, 255, 255]);
        let lit = canvas.pixels.chunks(4).filter(|px| px[3] == 255).count();
        assert_eq!(lit, 4);
    }

    #[test]
    fn blit_ignores_empty_canvas_and_masks() {
        let mut canvas = Canvas::new(0, 0);
        let mask = Mask {
            width: 0,
            height: 0,
            coverage: &[],
        };
        canvas.blit(&mask, (0, 0), [255, 255, 255, 255]);
        assert!(canvas.pixels.is_empty());
    }
}
