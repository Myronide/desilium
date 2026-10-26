//! Procedural linear gradients: linear-light blends with deterministic dithering.

use crate::color::{CHANNEL_MAX, linear_to_srgb, srgb_to_linear, to_byte};
use crate::geometry::{BYTES_PER_PIXEL, Size};

/// Entries in the precomputed color ramp; nearest-entry error stays well below one level.
const RAMP_SAMPLES: usize = 4096;
const HALF: f32 = 0.5;
const OPAQUE_ALPHA: f32 = 255.0;

// Interleaved gradient noise (Jimenez 2014): cheap, seed-free and evenly distributed.
const NOISE_SCALE: f32 = 52.982_918;
const NOISE_X_WEIGHT: f32 = 0.067_110_56;
const NOISE_Y_WEIGHT: f32 = 0.005_837_15;
/// Shifts each channel's noise pattern so that the channels do not dither in lockstep.
const CHANNEL_NOISE_SHIFT: f32 = 5.588_238;

/// A color at a position along the gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient, `0.0` at the start and `1.0` at the end; clamped to that range.
    pub position: f32,
    /// Straight (non-premultiplied) RGBA8 color.
    pub color: [u8; 4],
}

/// Renders a linear gradient into a tightly packed RGBA8 buffer.
///
/// `angle_degrees` is the direction of travel: `0` runs left to right and `90` runs top to bottom.
/// Colors are interpolated in linear light. `noise` in `0..=1` scales a dither that breaks up banding;
/// each channel moves by at most half a level times `noise`, so the result never strays more than one
/// level from the undithered value. Zero stops give transparent pixels and one stop gives a solid fill.
/// Returns an empty buffer for an empty output.
pub fn render_gradient(
    output: Size,
    stops: &[GradientStop],
    angle_degrees: f32,
    noise: f32,
) -> Vec<u8> {
    let Some(len) = output.rgba_len() else {
        return Vec::new();
    };
    if len == 0 {
        return Vec::new();
    }
    match stops {
        [] => vec![0; len],
        [only] => solid(only.color, len),
        _ => gradient(output, &sorted_stops(stops), angle_degrees, noise),
    }
}

fn solid(color: [u8; 4], len: usize) -> Vec<u8> {
    color.iter().cycle().take(len).copied().collect()
}

fn gradient(output: Size, stops: &[GradientStop], angle_degrees: f32, noise: f32) -> Vec<u8> {
    let ramp = build_ramp(stops);
    let axis = Axis::new(output, angle_degrees);
    let strength = if noise.is_finite() {
        noise.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let row_len = output.w as usize * BYTES_PER_PIXEL;
    let mut pixels = vec![0u8; output.rgba_len().unwrap_or(0)];
    for (y, row) in pixels.chunks_exact_mut(row_len.max(1)).enumerate() {
        for (x, pixel) in row
            .as_chunks_mut::<BYTES_PER_PIXEL>()
            .0
            .iter_mut()
            .enumerate()
        {
            let index = (axis.position(x, y) * (RAMP_SAMPLES - 1) as f32).round() as usize;
            let encoded = ramp.get(index).copied().unwrap_or([0.0; BYTES_PER_PIXEL]);
            write_dithered(pixel, encoded, (x, y), strength);
        }
    }
    pixels
}

/// Stores one pixel. Noise is centered on zero and at most half a level in size, before rounding.
fn write_dithered(
    pixel: &mut [u8],
    encoded: [f32; BYTES_PER_PIXEL],
    at: (usize, usize),
    strength: f32,
) {
    for (channel, (out, value)) in pixel.iter_mut().zip(encoded).enumerate() {
        let noise = (dither_unit(at.0, at.1, channel) - HALF) * strength;
        *out = to_byte(value + noise);
    }
}

/// Deterministic value in `0.0..1.0` derived from pixel coordinates and channel only.
fn dither_unit(x: usize, y: usize, channel: usize) -> f32 {
    let shifted_x = x as f32 + channel as f32 * CHANNEL_NOISE_SHIFT;
    let phase = NOISE_X_WEIGHT * shifted_x + NOISE_Y_WEIGHT * y as f32;
    (NOISE_SCALE * phase.fract()).fract()
}

/// Geometry of the gradient line for one output size and angle.
struct Axis {
    dir_x: f32,
    dir_y: f32,
    center_x: f32,
    center_y: f32,
    /// Length of the gradient line that spans the whole output box.
    extent: f32,
}

impl Axis {
    fn new(output: Size, angle_degrees: f32) -> Self {
        let angle = if angle_degrees.is_finite() {
            angle_degrees.to_radians()
        } else {
            0.0
        };
        let (sin, cos) = angle.sin_cos();
        let (w, h) = (output.w as f32, output.h as f32);
        Self {
            dir_x: cos,
            dir_y: sin,
            center_x: w * HALF,
            center_y: h * HALF,
            extent: w * cos.abs() + h * sin.abs(),
        }
    }

    /// Gradient position in `0.0..=1.0` for the center of pixel `(x, y)`; the box corners map to 0 and 1.
    fn position(&self, x: usize, y: usize) -> f32 {
        let dx = x as f32 + HALF - self.center_x;
        let dy = y as f32 + HALF - self.center_y;
        let projected = (dx * self.dir_x + dy * self.dir_y) / self.extent;
        (projected + HALF).clamp(0.0, 1.0)
    }
}

/// Stops with finite, clamped positions in ascending order.
fn sorted_stops(stops: &[GradientStop]) -> Vec<GradientStop> {
    let mut sorted: Vec<GradientStop> = stops
        .iter()
        .map(|stop| GradientStop {
            position: if stop.position.is_finite() {
                stop.position.clamp(0.0, 1.0)
            } else {
                0.0
            },
            color: stop.color,
        })
        .collect();
    sorted.sort_by(|a, b| a.position.total_cmp(&b.position));
    sorted
}

/// Precomputes the encoded sRGB color (0..=255 per channel) for evenly spaced positions.
fn build_ramp(stops: &[GradientStop]) -> Vec<[f32; BYTES_PER_PIXEL]> {
    let last = (RAMP_SAMPLES - 1) as f32;
    (0..RAMP_SAMPLES)
        .map(|index| encode(color_at(stops, index as f32 / last)))
        .collect()
}

/// Converts linear-light RGBA (alpha in 0..=1) into the stored 0..=255 sRGB representation.
fn encode([r, g, b, a]: [f32; 4]) -> [f32; BYTES_PER_PIXEL] {
    [
        linear_to_srgb(r) * CHANNEL_MAX,
        linear_to_srgb(g) * CHANNEL_MAX,
        linear_to_srgb(b) * CHANNEL_MAX,
        a.clamp(0.0, 1.0) * OPAQUE_ALPHA,
    ]
}

/// Interpolates the sorted stops at `position` in linear light.
fn color_at(stops: &[GradientStop], position: f32) -> [f32; 4] {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return [0.0; 4];
    };
    if position <= first.position {
        return linear_color(first.color);
    }
    if position >= last.position {
        return linear_color(last.color);
    }
    for pair in stops.windows(2) {
        if let [from, to] = pair
            && position <= to.position
        {
            let span = to.position - from.position;
            let t = if span > 0.0 {
                (position - from.position) / span
            } else {
                1.0
            };
            return lerp(linear_color(from.color), linear_color(to.color), t);
        }
    }
    linear_color(last.color)
}

fn linear_color([r, g, b, a]: [u8; 4]) -> [f32; 4] {
    [
        srgb_to_linear(r),
        srgb_to_linear(g),
        srgb_to_linear(b),
        f32::from(a) / OPAQUE_ALPHA,
    ]
}

fn lerp(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    let mut out = [0.0; 4];
    for ((slot, a), b) in out.iter_mut().zip(from).zip(to) {
        *slot = a + (b - a) * t;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn black_to_white() -> Vec<GradientStop> {
        vec![
            GradientStop {
                position: 0.0,
                color: BLACK,
            },
            GradientStop {
                position: 1.0,
                color: WHITE,
            },
        ]
    }

    fn row_red_values(buffer: &[u8], width: usize, row: usize) -> Vec<u8> {
        let start = row * width * 4;
        buffer[start..start + width * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| px[0])
            .collect()
    }

    #[test]
    fn endpoints_are_close_to_the_stop_colors() {
        let size = Size::new(256, 1);
        let buffer = render_gradient(size, &black_to_white(), 0.0, 0.0);
        let row = row_red_values(&buffer, 256, 0);
        assert!(row[0] <= 7, "first {}", row[0]);
        assert!(row[255] >= 248, "last {}", row[255]);
    }

    #[test]
    fn black_to_white_luminance_is_monotonic() {
        let size = Size::new(300, 2);
        let buffer = render_gradient(size, &black_to_white(), 0.0, 0.0);
        let row = row_red_values(&buffer, 300, 1);
        assert!(row.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn midpoint_is_blended_in_linear_light() {
        let buffer = render_gradient(Size::new(256, 1), &black_to_white(), 0.0, 0.0);
        let mid = buffer[128 * 4];
        // Linear half intensity encodes to about 188 in sRGB; a naive sRGB blend would give 128.
        assert!((i16::from(mid) - 188).abs() <= 3, "mid {mid}");
    }

    #[test]
    fn ninety_degrees_runs_top_to_bottom() {
        let size = Size::new(4, 64);
        let buffer = render_gradient(size, &black_to_white(), 90.0, 0.0);
        let top = buffer[0];
        let bottom = buffer[(63 * 4) * 4];
        // Pixel centers sit half a row inside the box, so the ends land at about 0.8% and 99.2%
        // of the ramp; in linear light that encodes to roughly 22 and 254.
        assert!(top < 24 && bottom > 245, "top {top} bottom {bottom}");
    }

    #[test]
    fn zero_stops_are_transparent_and_one_stop_is_solid() {
        let size = Size::new(3, 2);
        assert_eq!(render_gradient(size, &[], 0.0, 0.5), vec![0; 3 * 2 * 4]);
        let solid_stop = [GradientStop {
            position: 0.3,
            color: [9, 8, 7, 6],
        }];
        let buffer = render_gradient(size, &solid_stop, 45.0, 1.0);
        assert!(
            buffer
                .as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [9, 8, 7, 6])
        );
    }

    #[test]
    fn empty_output_yields_empty_buffer() {
        assert!(render_gradient(Size::new(0, 4), &black_to_white(), 0.0, 0.0).is_empty());
    }

    #[test]
    fn unsorted_stops_match_sorted_stops() {
        let sorted = vec![
            GradientStop {
                position: 0.0,
                color: [255, 0, 0, 255],
            },
            GradientStop {
                position: 0.5,
                color: [0, 255, 0, 255],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 255, 255],
            },
        ];
        let mut shuffled = sorted.clone();
        shuffled.reverse();
        let size = Size::new(40, 10);
        assert_eq!(
            render_gradient(size, &sorted, 30.0, 0.0),
            render_gradient(size, &shuffled, 30.0, 0.0)
        );
    }

    #[test]
    fn dither_stays_within_one_level_of_undithered_output() {
        let size = Size::new(128, 32);
        let plain = render_gradient(size, &black_to_white(), 20.0, 0.0);
        let dithered = render_gradient(size, &black_to_white(), 20.0, 1.0);
        let mut changed = 0usize;
        for (a, b) in plain.iter().zip(&dithered) {
            assert!((i16::from(*a) - i16::from(*b)).abs() <= 1, "{a} vs {b}");
            changed += usize::from(a != b);
        }
        assert!(changed > 0, "dither should alter at least some pixels");
    }

    #[test]
    fn same_input_gives_identical_bytes() {
        let size = Size::new(73, 41);
        let first = render_gradient(size, &black_to_white(), 133.0, 0.8);
        let second = render_gradient(size, &black_to_white(), 133.0, 0.8);
        assert_eq!(first, second);
    }

    #[test]
    fn alpha_interpolates_between_stops() {
        let stops = [
            GradientStop {
                position: 0.0,
                color: [0, 0, 0, 0],
            },
            GradientStop {
                position: 1.0,
                color: [0, 0, 0, 255],
            },
        ];
        let buffer = render_gradient(Size::new(256, 1), &stops, 0.0, 0.0);
        let alphas: Vec<u8> = buffer.as_chunks::<4>().0.iter().map(|px| px[3]).collect();
        assert!(alphas.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(alphas[0] < 8 && alphas[255] > 247);
    }
}
