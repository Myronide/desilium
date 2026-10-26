//! Separable RGBA8 resampling: Lanczos3 when shrinking, bilinear when enlarging.

use std::f32::consts::PI;

use crate::color::to_byte;
use crate::geometry::BYTES_PER_PIXEL;

/// Lobes of the Lanczos kernel; three is the usual quality/cost trade-off.
const LANCZOS_LOBES: f32 = 3.0;
/// Support of the bilinear kernel in source samples.
const BILINEAR_RADIUS: f32 = 1.0;
/// Weight sums below this are treated as degenerate and fall back to nearest sampling.
const WEIGHT_EPSILON: f32 = 1e-6;
/// Pixel centers sit half a sample off the integer grid.
const HALF: f32 = 0.5;

/// Source samples that contribute to one destination sample along one axis.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tap {
    start: usize,
    weights: Vec<f32>,
}

/// Maps `dst_len` samples onto the source window `origin..origin + span` along one axis.
///
/// Returns no taps for empty input or a non-positive span; the caller treats that as nothing to draw.
pub(crate) fn build_taps(src_len: usize, origin: f32, span: f32, dst_len: usize) -> Vec<Tap> {
    if src_len == 0 || dst_len == 0 || !span.is_finite() || span <= 0.0 {
        return Vec::new();
    }
    let ratio = span / dst_len as f32;
    let shrinking = ratio > 1.0;
    let kernel_scale = if shrinking { ratio } else { 1.0 };
    let last = (src_len - 1) as f32;
    (0..dst_len)
        .map(|index| {
            let center = origin + (index as f32 + HALF) * ratio - HALF;
            tap_at(center, last, kernel_scale, shrinking)
        })
        .collect()
}

/// Builds the taps for one destination sample centered at `center` in source coordinates.
fn tap_at(center: f32, last: f32, kernel_scale: f32, shrinking: bool) -> Tap {
    let lobes = if shrinking {
        LANCZOS_LOBES
    } else {
        BILINEAR_RADIUS
    };
    let radius = lobes * kernel_scale;
    let start = (center - radius).floor().clamp(0.0, last);
    let end = (center + radius).ceil().clamp(0.0, last);
    let (start, end) = (start as usize, end as usize);
    let weights: Vec<f32> = (start..=end)
        .map(|src| kernel((src as f32 - center) / kernel_scale, shrinking))
        .collect();
    let nearest = center.round().clamp(0.0, last) as usize;
    normalized(start, weights, nearest)
}

fn normalized(start: usize, weights: Vec<f32>, nearest: usize) -> Tap {
    let sum: f32 = weights.iter().sum();
    if sum.abs() < WEIGHT_EPSILON {
        return Tap {
            start: nearest,
            weights: vec![1.0],
        };
    }
    Tap {
        start,
        weights: weights.into_iter().map(|weight| weight / sum).collect(),
    }
}

/// Kernel value at a distance already expressed in kernel units.
fn kernel(distance: f32, shrinking: bool) -> f32 {
    if shrinking {
        return lanczos(distance);
    }
    (1.0 - distance.abs()).max(0.0)
}

fn lanczos(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude < WEIGHT_EPSILON {
        return 1.0;
    }
    if magnitude >= LANCZOS_LOBES {
        return 0.0;
    }
    let scaled = PI * x;
    LANCZOS_LOBES * scaled.sin() * (scaled / LANCZOS_LOBES).sin() / (scaled * scaled)
}

/// Resamples an RGBA8 raster with the given per-axis taps.
///
/// The axis that shrinks the buffer is filtered first, which keeps the intermediate buffer as small as possible.
pub(crate) fn resample(
    data: &[u8],
    width: usize,
    height: usize,
    x_taps: &[Tap],
    y_taps: &[Tap],
) -> Vec<u8> {
    let horizontal_first = x_taps.len() * height <= width * y_taps.len();
    if horizontal_first {
        let mid = filter_horizontal(data, width, height, x_taps);
        return filter_vertical(&mid, x_taps.len(), y_taps);
    }
    let mid = filter_vertical(data, width, y_taps);
    filter_horizontal(&mid, width, y_taps.len(), x_taps)
}

fn filter_horizontal(data: &[u8], width: usize, height: usize, taps: &[Tap]) -> Vec<u8> {
    let src_row_len = (width * BYTES_PER_PIXEL).max(1);
    let out_row_len = taps.len() * BYTES_PER_PIXEL;
    let mut out = vec![0u8; out_row_len * height];
    let rows = data
        .chunks_exact(src_row_len)
        .zip(out.chunks_exact_mut(out_row_len.max(1)));
    for (src_row, out_row) in rows {
        for (pixel, tap) in out_row
            .as_chunks_mut::<BYTES_PER_PIXEL>()
            .0
            .iter_mut()
            .zip(taps)
        {
            pixel.copy_from_slice(&accumulate_row(src_row, tap));
        }
    }
    out
}

fn accumulate_row(src_row: &[u8], tap: &Tap) -> [u8; BYTES_PER_PIXEL] {
    let mut acc = [0f32; BYTES_PER_PIXEL];
    let Some(window) = src_row.get(tap.start * BYTES_PER_PIXEL..) else {
        return [0; BYTES_PER_PIXEL];
    };
    for (source, weight) in window
        .as_chunks::<BYTES_PER_PIXEL>()
        .0
        .iter()
        .zip(&tap.weights)
    {
        for (sum, &value) in acc.iter_mut().zip(source) {
            *sum += f32::from(value) * weight;
        }
    }
    acc.map(to_byte)
}

fn filter_vertical(data: &[u8], width: usize, taps: &[Tap]) -> Vec<u8> {
    let row_len = width * BYTES_PER_PIXEL;
    let mut out = vec![0u8; row_len * taps.len()];
    let mut acc = vec![0f32; row_len];
    for (out_row, tap) in out.chunks_exact_mut(row_len.max(1)).zip(taps) {
        acc.fill(0.0);
        accumulate_rows(data, row_len, tap, &mut acc);
        for (dst, &sum) in out_row.iter_mut().zip(&acc) {
            *dst = to_byte(sum);
        }
    }
    out
}

fn accumulate_rows(data: &[u8], row_len: usize, tap: &Tap, acc: &mut [f32]) {
    for (offset, weight) in tap.weights.iter().enumerate() {
        let row_start = (tap.start + offset) * row_len;
        let Some(row) = data.get(row_start..row_start + row_len) else {
            continue;
        };
        for (sum, &value) in acc.iter_mut().zip(row) {
            *sum += f32::from(value) * weight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant_raster(width: usize, height: usize, value: u8) -> Vec<u8> {
        vec![value; width * height * BYTES_PER_PIXEL]
    }

    #[test]
    fn identity_scale_is_exact() {
        let data: Vec<u8> = (0..4 * 3 * 4).map(|v| v as u8).collect();
        let x = build_taps(4, 0.0, 4.0, 4);
        let y = build_taps(3, 0.0, 3.0, 3);
        assert_eq!(resample(&data, 4, 3, &x, &y), data);
    }

    #[test]
    fn constant_image_stays_constant_when_shrinking_and_enlarging() {
        let data = constant_raster(16, 16, 77);
        for dst in [4usize, 9, 40] {
            let x = build_taps(16, 0.0, 16.0, dst);
            let y = build_taps(16, 0.0, 16.0, dst);
            let out = resample(&data, 16, 16, &x, &y);
            assert!(out.iter().all(|&v| v == 77), "dst {dst}");
        }
    }

    #[test]
    fn taps_are_normalized_and_stay_in_bounds() {
        for tap in build_taps(10, -2.0, 14.0, 5) {
            let sum: f32 = tap.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-4);
            assert!(tap.start + tap.weights.len() <= 10);
        }
    }

    #[test]
    fn invalid_windows_produce_no_taps() {
        assert!(build_taps(0, 0.0, 1.0, 1).is_empty());
        assert!(build_taps(4, 0.0, 0.0, 1).is_empty());
        assert!(build_taps(4, 0.0, f32::NAN, 1).is_empty());
        assert!(build_taps(4, 0.0, 4.0, 0).is_empty());
    }

    #[test]
    fn halving_a_step_edge_keeps_both_sides() {
        // Left half black, right half white. Shrinking uses a kernel three output samples wide,
        // so the edge pixels stay mostly on their own side (with a little Lanczos ringing).
        let mut data = Vec::new();
        for _ in 0..8 {
            for x in 0..8 {
                let value = if x < 4 { 0 } else { 255 };
                data.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let x = build_taps(8, 0.0, 8.0, 2);
        let y = build_taps(8, 0.0, 8.0, 1);
        let out = resample(&data, 8, 8, &x, &y);
        assert!(out[0] < 40 && out[4] > 215, "{out:?}");
    }
}
