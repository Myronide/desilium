//! Pure fit math that maps a source image onto an output surface.

/// Bytes per RGBA8 pixel.
pub(crate) const BYTES_PER_PIXEL: usize = 4;

/// Divisor used to center a rectangle inside another one.
const CENTER_DIVISOR: u32 = 2;

/// Width and height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

impl Size {
    /// Creates a size from width and height.
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }

    /// True when either dimension is zero.
    pub const fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Byte length of a tightly packed RGBA8 buffer, or `None` if it does not fit in `usize`.
    pub fn rgba_len(self) -> Option<usize> {
        let pixels = u64::from(self.w).checked_mul(u64::from(self.h))?;
        let bytes = pixels.checked_mul(BYTES_PER_PIXEL as u64)?;
        usize::try_from(bytes).ok()
    }
}

/// Rectangle with fractional coordinates, used for source regions that may fall between pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Integer rectangle in output pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RectI {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl RectI {
    /// Rectangle of `w` by `h` centered inside `outer`.
    fn centered(outer: Size, w: u32, h: u32) -> Self {
        Self {
            x: to_i32(centered_offset(outer.w, w)),
            y: to_i32(centered_offset(outer.h, h)),
            w,
            h,
        }
    }
}

/// How an image is placed on an output surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// Cover the output, cropping the overflowing axis around the center.
    Fill,
    /// Fit entirely inside the output, leaving letterbox or pillarbox bars.
    Fit,
    /// Scale to the output size, ignoring the aspect ratio.
    Stretch,
    /// Draw at native scale, centered; crop when the image is larger than the output.
    Center,
    /// Repeat the image at native scale from the top-left corner.
    Tile,
}

/// Where a source region lands on the output.
///
/// For [`FitMode::Tile`] `src` covers the whole image and `dst` the whole output; the renderer repeats the image across it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Placement {
    /// Source region in image pixels.
    pub src: RectF,
    /// Destination region in output pixels.
    pub dst: RectI,
}

/// Computes how `image` is placed on `output` for the given mode.
///
/// Returns an empty placement when either size has a zero dimension.
pub fn fit_placement(image: Size, output: Size, mode: FitMode) -> Placement {
    if image.is_empty() || output.is_empty() {
        return Placement::default();
    }
    match mode {
        FitMode::Fill => fill(image, output),
        FitMode::Fit => fit(image, output),
        FitMode::Stretch | FitMode::Tile => Placement {
            src: whole_image(image),
            dst: whole_output(output),
        },
        FitMode::Center => center(image, output),
    }
}

/// True when the image is proportionally wider than the output (compared exactly in integers).
fn image_is_wider(image: Size, output: Size) -> bool {
    u64::from(image.w) * u64::from(output.h) > u64::from(image.h) * u64::from(output.w)
}

fn fill(image: Size, output: Size) -> Placement {
    let (crop_w, crop_h) = if image_is_wider(image, output) {
        (scaled(image.h, output.w, output.h), f64::from(image.h))
    } else {
        (f64::from(image.w), scaled(image.w, output.h, output.w))
    };
    let src = RectF {
        x: ((f64::from(image.w) - crop_w) / 2.0) as f32,
        y: ((f64::from(image.h) - crop_h) / 2.0) as f32,
        w: crop_w as f32,
        h: crop_h as f32,
    };
    Placement {
        src,
        dst: whole_output(output),
    }
}

fn fit(image: Size, output: Size) -> Placement {
    let dst = if image_is_wider(image, output) {
        let h = scaled_round(image.h, output.w, image.w).clamp(1, output.h);
        RectI::centered(output, output.w, h)
    } else {
        let w = scaled_round(image.w, output.h, image.h).clamp(1, output.w);
        RectI::centered(output, w, output.h)
    };
    Placement {
        src: whole_image(image),
        dst,
    }
}

fn center(image: Size, output: Size) -> Placement {
    let crop = Size::new(image.w.min(output.w), image.h.min(output.h));
    let src = RectF {
        x: centered_offset(image.w, crop.w) as f32,
        y: centered_offset(image.h, crop.h) as f32,
        w: crop.w as f32,
        h: crop.h as f32,
    };
    Placement {
        src,
        dst: RectI::centered(output, crop.w, crop.h),
    }
}

fn whole_image(image: Size) -> RectF {
    RectF {
        x: 0.0,
        y: 0.0,
        w: image.w as f32,
        h: image.h as f32,
    }
}

fn whole_output(output: Size) -> RectI {
    RectI {
        x: 0,
        y: 0,
        w: output.w,
        h: output.h,
    }
}

/// `len * num / den` in floating point; exact enough for pixel coordinates up to `u32::MAX`.
fn scaled(len: u32, num: u32, den: u32) -> f64 {
    f64::from(len) * f64::from(num) / f64::from(den)
}

/// `len * num / den` rounded to the nearest integer, saturating at `u32::MAX`.
fn scaled_round(len: u32, num: u32, den: u32) -> u32 {
    let half = u64::from(den) / u64::from(CENTER_DIVISOR);
    let rounded = (u64::from(len) * u64::from(num) + half) / u64::from(den);
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

fn centered_offset(outer: u32, inner: u32) -> u32 {
    outer.saturating_sub(inner) / CENTER_DIVISOR
}

fn to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-3;

    fn assert_src(actual: RectF, x: f32, y: f32, w: f32, h: f32) {
        let expected = [x, y, w, h];
        let actual = [actual.x, actual.y, actual.w, actual.h];
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < EPSILON, "src {actual:?} != {expected:?}");
        }
    }

    fn dst(x: i32, y: i32, w: u32, h: u32) -> RectI {
        RectI { x, y, w, h }
    }

    const HD: Size = Size::new(1920, 1080);
    const FOUR_THREE: Size = Size::new(1024, 768);

    #[test]
    fn fill_16x9_into_4x3_crops_the_sides() {
        let p = fit_placement(HD, FOUR_THREE, FitMode::Fill);
        assert_src(p.src, 240.0, 0.0, 1440.0, 1080.0);
        assert_eq!(p.dst, dst(0, 0, 1024, 768));
    }

    #[test]
    fn fill_4x3_into_16x9_crops_top_and_bottom() {
        let p = fit_placement(FOUR_THREE, HD, FitMode::Fill);
        assert_src(p.src, 0.0, 96.0, 1024.0, 576.0);
        assert_eq!(p.dst, dst(0, 0, 1920, 1080));
    }

    #[test]
    fn fit_16x9_into_4x3_letterboxes() {
        let p = fit_placement(HD, FOUR_THREE, FitMode::Fit);
        assert_src(p.src, 0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(p.dst, dst(0, 96, 1024, 576));
    }

    #[test]
    fn fit_4x3_into_16x9_pillarboxes() {
        let p = fit_placement(FOUR_THREE, HD, FitMode::Fit);
        assert_src(p.src, 0.0, 0.0, 1024.0, 768.0);
        assert_eq!(p.dst, dst(240, 0, 1440, 1080));
    }

    #[test]
    fn fit_same_aspect_fills_output() {
        let p = fit_placement(Size::new(800, 600), Size::new(1600, 1200), FitMode::Fit);
        assert_eq!(p.dst, dst(0, 0, 1600, 1200));
    }

    #[test]
    fn stretch_covers_output_ignoring_aspect() {
        let p = fit_placement(HD, FOUR_THREE, FitMode::Stretch);
        assert_src(p.src, 0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(p.dst, dst(0, 0, 1024, 768));
    }

    #[test]
    fn center_smaller_image_is_centered_without_scaling() {
        let p = fit_placement(Size::new(100, 50), Size::new(400, 300), FitMode::Center);
        assert_src(p.src, 0.0, 0.0, 100.0, 50.0);
        assert_eq!(p.dst, dst(150, 125, 100, 50));
    }

    #[test]
    fn center_larger_image_crops_to_centered_window() {
        let p = fit_placement(Size::new(800, 600), Size::new(400, 300), FitMode::Center);
        assert_src(p.src, 200.0, 150.0, 400.0, 300.0);
        assert_eq!(p.dst, dst(0, 0, 400, 300));
    }

    #[test]
    fn center_handles_mixed_axes() {
        let p = fit_placement(Size::new(800, 100), Size::new(400, 300), FitMode::Center);
        assert_src(p.src, 200.0, 0.0, 400.0, 100.0);
        assert_eq!(p.dst, dst(0, 100, 400, 100));
    }

    #[test]
    fn tile_uses_whole_image_and_output() {
        let p = fit_placement(Size::new(64, 64), HD, FitMode::Tile);
        assert_src(p.src, 0.0, 0.0, 64.0, 64.0);
        assert_eq!(p.dst, dst(0, 0, 1920, 1080));
    }

    #[test]
    fn zero_sizes_produce_empty_placement_in_every_mode() {
        let modes = [
            FitMode::Fill,
            FitMode::Fit,
            FitMode::Stretch,
            FitMode::Center,
            FitMode::Tile,
        ];
        for mode in modes {
            assert_eq!(
                fit_placement(Size::new(0, 10), HD, mode),
                Placement::default()
            );
            assert_eq!(
                fit_placement(HD, Size::new(10, 0), mode),
                Placement::default()
            );
        }
    }

    #[test]
    fn fit_never_collapses_to_zero_extent() {
        let p = fit_placement(Size::new(1, 100_000), Size::new(10, 10), FitMode::Fit);
        assert_eq!(p.dst.w, 1);
        assert_eq!(p.dst.h, 10);
    }

    #[test]
    fn huge_sizes_do_not_overflow() {
        let huge = Size::new(u32::MAX, u32::MAX);
        let fill = fit_placement(huge, HD, FitMode::Fill);
        assert_eq!(fill.dst, dst(0, 0, 1920, 1080));
        let fit = fit_placement(Size::new(1, u32::MAX), HD, FitMode::Fit);
        assert_eq!(fit.dst.h, 1080);
        let center = fit_placement(huge, HD, FitMode::Center);
        assert_eq!(center.dst, dst(0, 0, 1920, 1080));
    }

    #[test]
    fn placements_stay_within_image_and_output_bounds() {
        let sizes = [
            Size::new(1, 1),
            Size::new(3, 7),
            Size::new(1920, 1080),
            Size::new(1000, 3),
            Size::new(17, 4096),
        ];
        let modes = [
            FitMode::Fill,
            FitMode::Fit,
            FitMode::Stretch,
            FitMode::Center,
            FitMode::Tile,
        ];
        for image in sizes {
            for output in sizes {
                for mode in modes {
                    assert_in_bounds(image, output, mode);
                }
            }
        }
    }

    fn assert_in_bounds(image: Size, output: Size, mode: FitMode) {
        let p = fit_placement(image, output, mode);
        let tolerance = 1e-2;
        assert!(p.src.x >= -tolerance && p.src.y >= -tolerance);
        assert!(p.src.x + p.src.w <= image.w as f32 + tolerance, "{mode:?}");
        assert!(p.src.y + p.src.h <= image.h as f32 + tolerance, "{mode:?}");
        assert!(p.dst.x >= 0 && p.dst.y >= 0);
        assert!(p.dst.x as u64 + u64::from(p.dst.w) <= u64::from(output.w));
        assert!(p.dst.y as u64 + u64::from(p.dst.h) <= u64::from(output.h));
    }

    #[test]
    fn rgba_len_reports_overflow_as_none() {
        assert_eq!(Size::new(2, 3).rgba_len(), Some(24));
        assert_eq!(Size::new(u32::MAX, u32::MAX).rgba_len(), None);
    }
}
