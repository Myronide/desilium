//! Pure geometry of the themed cursor: image scale choice and pixel-snapped placement.

/// Integer scale used to pick a cursor image; fractional outputs round up and downscale.
pub fn image_scale(output_scale: f64) -> u32 {
    let rounded = output_scale.ceil();
    if !rounded.is_finite() || rounded < 1.0 {
        return 1;
    }
    rounded as u32
}

/// Physical top-left of the cursor bitmap, snapped to whole pixels so it stays crisp.
///
/// The bitmap was made for `image_scale` but is shown at `output_scale`, which scales the
/// hotspot by the same ratio.
pub fn snapped_origin(
    pointer: (f64, f64),
    output_scale: f64,
    hotspot: (u32, u32),
    image_scale: u32,
) -> (f64, f64) {
    let ratio = output_scale / f64::from(image_scale.max(1));
    let x = pointer.0 * output_scale - f64::from(hotspot.0) * ratio;
    let y = pointer.1 * output_scale - f64::from(hotspot.1) * ratio;
    (x.round(), y.round())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_scale_rounds_up() {
        assert_eq!(image_scale(1.0), 1);
        assert_eq!(image_scale(1.25), 2);
        assert_eq!(image_scale(2.0), 2);
        assert_eq!(image_scale(0.0), 1);
        assert_eq!(image_scale(f64::NAN), 1);
    }

    #[test]
    fn origin_subtracts_the_hotspot_and_snaps() {
        assert_eq!(snapped_origin((100.4, 50.6), 1.0, (3, 2), 1), (97.0, 49.0));
    }

    #[test]
    fn hotspot_follows_the_display_ratio() {
        // 2x bitmap shown at 1.5x: hotspot of 8 image pixels is 6 physical pixels.
        assert_eq!(snapped_origin((10.0, 10.0), 1.5, (8, 8), 2), (9.0, 9.0));
    }

    #[test]
    fn integer_scale_keeps_hotspot_in_image_pixels() {
        assert_eq!(snapped_origin((10.0, 10.0), 2.0, (8, 4), 2), (12.0, 16.0));
    }
}
