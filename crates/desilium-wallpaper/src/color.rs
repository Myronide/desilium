//! sRGB transfer functions and image color statistics.

use crate::geometry::{BYTES_PER_PIXEL, Size};

/// Full-scale value of an 8-bit channel.
pub(crate) const CHANNEL_MAX: f32 = 255.0;

/// Samples taken per axis by [`average_color`]; enough for a stable accent color.
const SAMPLES_PER_AXIS: u32 = 256;
/// Encoded sRGB value below which the decode curve is linear.
const DECODE_CUTOFF: f32 = 0.040_45;
/// Linear value below which the encode curve is linear.
const ENCODE_CUTOFF: f32 = 0.003_130_8;
const LINEAR_SLOPE: f32 = 12.92;
const GAMMA: f32 = 2.4;
const CURVE_SCALE: f32 = 1.055;
const CURVE_OFFSET: f32 = 0.055;

/// Decodes an sRGB byte to linear light in `0.0..=1.0`.
pub(crate) fn srgb_to_linear(channel: u8) -> f32 {
    let encoded = f32::from(channel) / CHANNEL_MAX;
    if encoded <= DECODE_CUTOFF {
        return encoded / LINEAR_SLOPE;
    }
    ((encoded + CURVE_OFFSET) / CURVE_SCALE).powf(GAMMA)
}

/// Encodes linear light to sRGB in `0.0..=1.0`; input is clamped first.
pub(crate) fn linear_to_srgb(linear: f32) -> f32 {
    let clamped = linear.clamp(0.0, 1.0);
    if clamped <= ENCODE_CUTOFF {
        return clamped * LINEAR_SLOPE;
    }
    CURVE_SCALE * clamped.powf(1.0 / GAMMA) - CURVE_OFFSET
}

/// Rounds a `0.0..=255.0` value to a byte, saturating out-of-range and NaN input to the nearest bound.
pub(crate) fn to_byte(value: f32) -> u8 {
    value.round().clamp(0.0, CHANNEL_MAX) as u8
}

/// Mean color of an RGBA8 buffer, sampled on a coarse grid.
///
/// The mean is taken on the stored (sRGB) byte values, which is what an accent-color heuristic
/// expects. Returns transparent black when the buffer is shorter than `size` or the size is empty.
pub fn average_color(rgba: &[u8], size: Size) -> [u8; 4] {
    let Some(len) = size.rgba_len() else {
        return [0; BYTES_PER_PIXEL];
    };
    let Some(pixels) = rgba.get(..len) else {
        return [0; BYTES_PER_PIXEL];
    };
    if size.is_empty() {
        return [0; BYTES_PER_PIXEL];
    }
    let step_x = (size.w / SAMPLES_PER_AXIS).max(1) as usize;
    let step_y = (size.h / SAMPLES_PER_AXIS).max(1) as usize;
    let (sums, count) = sum_samples(pixels, size, step_x, step_y);
    if count == 0 {
        return [0; BYTES_PER_PIXEL];
    }
    sums.map(|sum| u8::try_from((sum + count / 2) / count).unwrap_or(u8::MAX))
}

fn sum_samples(pixels: &[u8], size: Size, step_x: usize, step_y: usize) -> ([u64; 4], u64) {
    let row_len = size.w as usize * BYTES_PER_PIXEL;
    let mut sums = [0u64; BYTES_PER_PIXEL];
    let mut count = 0u64;
    for row in (0..size.h as usize).step_by(step_y) {
        let Some(line) = pixels.get(row * row_len..(row + 1) * row_len) else {
            continue;
        };
        for pixel in line.as_chunks::<BYTES_PER_PIXEL>().0.iter().step_by(step_x) {
            for (sum, &value) in sums.iter_mut().zip(pixel) {
                *sum += u64::from(value);
            }
            count += 1;
        }
    }
    (sums, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_transfer_round_trips_every_byte() {
        for channel in 0..=255u8 {
            let linear = srgb_to_linear(channel);
            let back = linear_to_srgb(linear) * CHANNEL_MAX;
            assert!(
                (back - f32::from(channel)).abs() < 1e-2,
                "channel {channel}"
            );
        }
    }

    #[test]
    fn srgb_midpoint_is_darker_than_linear_midpoint() {
        // Half intensity in linear light is about 188/255 in sRGB, not 128.
        let encoded = linear_to_srgb(0.5) * CHANNEL_MAX;
        assert!((encoded - 188.0).abs() < 1.0, "{encoded}");
    }

    #[test]
    fn uniform_image_averages_to_itself() {
        let pixels = [10, 20, 30, 255].repeat(64);
        assert_eq!(average_color(&pixels, Size::new(8, 8)), [10, 20, 30, 255]);
    }

    #[test]
    fn half_black_half_white_averages_to_mid_gray() {
        let mut pixels = Vec::new();
        for _ in 0..4 {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        }
        for _ in 0..4 {
            pixels.extend_from_slice(&[255, 255, 255, 255]);
        }
        assert_eq!(
            average_color(&pixels, Size::new(4, 2)),
            [128, 128, 128, 255]
        );
    }

    #[test]
    fn short_buffer_or_empty_size_yields_transparent_black() {
        assert_eq!(average_color(&[1, 2, 3], Size::new(1, 1)), [0; 4]);
        assert_eq!(average_color(&[], Size::new(0, 4)), [0; 4]);
    }

    #[test]
    fn large_image_is_sampled_and_stays_accurate() {
        let size = Size::new(2000, 1000);
        let pixels = [200, 100, 50, 255].repeat(2000 * 1000);
        assert_eq!(average_color(&pixels, size), [200, 100, 50, 255]);
    }
}
