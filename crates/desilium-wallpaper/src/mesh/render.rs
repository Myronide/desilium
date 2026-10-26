//! Per-pixel evaluation of a [`MeshSpec`], parallel over bands of rows.

use super::noise::{fbm, white};
use super::{Blob, MeshSpec};
use crate::color::{linear_to_srgb, srgb_to_linear, to_byte};
use crate::geometry::{BYTES_PER_PIXEL, Size};
use std::thread;

const CHANNELS: usize = 3;
const OPAQUE: u8 = u8::MAX;
const CHANNEL_SCALE: f32 = 255.0;
/// Spatial frequency of the warp noise, in cells per image height.
const WARP_FREQUENCY: f32 = 1.6;
const WARP_SEED_X: u64 = 0x51;
const WARP_SEED_Y: u64 = 0xa7;
/// Gaussian falloff steepness: weight is `exp(-FALLOFF * d^2)` with `d` in blob radii.
const FALLOFF: f32 = 2.6;
/// Salts that decorrelate the grain of each channel and its luminance component.
const SALT_RED: u64 = 11;
const SALT_GREEN: u64 = 23;
const SALT_BLUE: u64 = 37;
const SALT_LUMA: u64 = 53;
/// Offset that decorrelates the two uniform samples of one triangular draw.
const SECOND_SAMPLE_SALT: u64 = 1000;
/// Share of the grain that is shared by all channels (luminance grain looks filmic, chroma grain digital).
const LUMA_GRAIN_SHARE: f32 = 0.75;
/// Standard deviation of a sum of two uniform variables minus their mean, in uniform widths.
const TRIANGULAR_NORM: f32 = 2.449_489_7;
const MAX_ROWS_PER_BAND: usize = 64;

struct Prepared {
    base: [[f32; CHANNELS]; 2],
    blobs: Vec<PreparedBlob>,
    warp: f32,
    vignette: f32,
    grain: f32,
    seed: u64,
    width: f32,
    height: f32,
}

struct PreparedBlob {
    center: (f32, f32),
    inv_radius: f32,
    inv_stretch: f32,
    axis: (f32, f32),
    color: [f32; CHANNELS],
    strength: f32,
}

/// Renders the spec into an opaque RGBA8 buffer; empty when the size is empty or too large.
pub fn render_mesh(size: Size, spec: &MeshSpec, seed: u64) -> Vec<u8> {
    let Some(len) = size.rgba_len().filter(|len| *len > 0) else {
        return Vec::new();
    };
    let prepared = Prepared::new(size, spec, seed);
    let mut pixels = vec![0u8; len];
    let row_len = size.w as usize * BYTES_PER_PIXEL;
    let workers = thread::available_parallelism().map_or(1, usize::from);
    let band_rows = (size.h as usize / workers.max(1)).clamp(1, MAX_ROWS_PER_BAND * 8);
    thread::scope(|scope| {
        for (band, rows) in pixels.chunks_mut(row_len * band_rows).enumerate() {
            let prepared = &prepared;
            scope.spawn(move || prepared.fill_band(rows, row_len, band * band_rows));
        }
    });
    pixels
}

impl Prepared {
    fn new(size: Size, spec: &MeshSpec, seed: u64) -> Self {
        let linear = |c: [u8; 3]| c.map(srgb_to_linear);
        Self {
            base: [linear(spec.base[0]), linear(spec.base[1])],
            blobs: spec
                .blobs
                .iter()
                .map(|blob| PreparedBlob::new(blob, size))
                .collect(),
            warp: spec.warp,
            vignette: spec.vignette,
            grain: spec.grain,
            seed,
            width: size.w as f32,
            height: size.h as f32,
        }
    }

    fn fill_band(&self, rows: &mut [u8], row_len: usize, first_row: usize) {
        for (offset, row) in rows.chunks_exact_mut(row_len.max(1)).enumerate() {
            let y = first_row + offset;
            for (x, pixel) in row
                .as_chunks_mut::<BYTES_PER_PIXEL>()
                .0
                .iter_mut()
                .enumerate()
            {
                *pixel = self.pixel(x, y);
            }
        }
    }

    fn pixel(&self, x: usize, y: usize) -> [u8; BYTES_PER_PIXEL] {
        // Unit square scaled by height so that glows stay round on wide images.
        let aspect = self.width / self.height;
        let (u, v) = (x as f32 / self.width, y as f32 / self.height);
        let warped = self.warped(u * aspect, v);
        let mut color = mix(self.base[0], self.base[1], v);
        for blob in &self.blobs {
            let weight = blob.weight(warped);
            color = mix(color, blob.color, weight);
        }
        let shade = self.shade(u, v);
        let grain = self.grain_at(x, y);
        let [r, g, b] = color.map(|c| linear_to_srgb(c * shade) * CHANNEL_SCALE);
        [
            to_byte(r + grain[0]),
            to_byte(g + grain[1]),
            to_byte(b + grain[2]),
            OPAQUE,
        ]
    }

    fn warped(&self, px: f32, py: f32) -> (f32, f32) {
        let dx = fbm(self.seed ^ WARP_SEED_X, px, py, WARP_FREQUENCY) - 0.5;
        let dy = fbm(self.seed ^ WARP_SEED_Y, px, py, WARP_FREQUENCY) - 0.5;
        (px + dx * self.warp * 2.0, py + dy * self.warp * 2.0)
    }

    /// Linear-light multiplier that darkens the corners.
    fn shade(&self, u: f32, v: f32) -> f32 {
        let (dx, dy) = (u - 0.5, v - 0.5);
        let distance_sq = (dx * dx + dy * dy) * 2.0;
        1.0 - self.vignette * distance_sq.min(1.0)
    }

    /// Triangular-distribution grain in 8-bit levels, mostly shared between channels.
    fn grain_at(&self, x: usize, y: usize) -> [f32; CHANNELS] {
        let triangular = |salt: u64| {
            let sum = white(self.seed, x, y, salt)
                + white(self.seed, x, y, salt + SECOND_SAMPLE_SALT)
                - 1.0;
            sum * TRIANGULAR_NORM * self.grain / 2.0
        };
        let luma = triangular(SALT_LUMA);
        let mixed =
            |salt: u64| luma * LUMA_GRAIN_SHARE + triangular(salt) * (1.0 - LUMA_GRAIN_SHARE);
        [mixed(SALT_RED), mixed(SALT_GREEN), mixed(SALT_BLUE)]
    }
}

impl PreparedBlob {
    fn new(blob: &Blob, size: Size) -> Self {
        let angle = blob.angle_deg.to_radians();
        let aspect = size.w as f32 / (size.h as f32).max(1.0);
        Self {
            center: (blob.center.0 * aspect, blob.center.1),
            inv_radius: 1.0 / blob.radius.max(f32::EPSILON),
            inv_stretch: 1.0 / blob.stretch.max(f32::EPSILON),
            axis: (angle.cos(), angle.sin()),
            color: blob.color.map(srgb_to_linear),
            strength: blob.strength,
        }
    }

    /// Opacity at a point in height-normalized coordinates.
    fn weight(&self, point: (f32, f32)) -> f32 {
        let dx = point.0 - self.center.0;
        let dy = point.1 - self.center.1;
        let along = (dx * self.axis.0 + dy * self.axis.1) * self.inv_stretch;
        let across = -dx * self.axis.1 + dy * self.axis.0;
        let distance_sq = (along * along + across * across) * self.inv_radius * self.inv_radius;
        self.strength * (-FALLOFF * distance_sq).exp()
    }
}

fn mix(from: [f32; CHANNELS], to: [f32; CHANNELS], t: f32) -> [f32; CHANNELS] {
    [
        from[0] + (to[0] - from[0]) * t,
        from[1] + (to[1] - from[1]) * t,
        from[2] + (to[2] - from[2]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::preset;

    const SMALL: Size = Size::new(160, 90);

    fn render(name: &str, seed: u64) -> Vec<u8> {
        render_mesh(SMALL, &preset(name).unwrap(), seed)
    }

    #[test]
    fn output_has_requested_size_and_is_opaque() {
        let pixels = render("dusk-violet", 1);
        assert_eq!(pixels.len(), 160 * 90 * 4);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn same_seed_is_identical_and_other_seed_differs() {
        assert_eq!(render("aurora", 5), render("aurora", 5));
        assert_ne!(render("aurora", 5), render("aurora", 6));
    }

    #[test]
    fn empty_size_gives_empty_buffer() {
        let spec = preset("slate").unwrap();
        assert!(render_mesh(Size::new(0, 10), &spec, 1).is_empty());
    }

    #[test]
    fn every_preset_has_a_wide_histogram() {
        for name in crate::mesh::PRESET_NAMES {
            let pixels = render(name, 1);
            let mut histogram = [0u32; 256];
            for pixel in pixels.as_chunks::<4>().0 {
                let luma =
                    (u32::from(pixel[0]) * 3 + u32::from(pixel[1]) * 6 + u32::from(pixel[2])) / 10;
                histogram[luma as usize] += 1;
            }
            let used = histogram.iter().filter(|count| **count > 0).count();
            assert!(used > 25, "{name} uses only {used} luma levels");
        }
    }
}
