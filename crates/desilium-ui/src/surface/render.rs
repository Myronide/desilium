//! Rasterisation into a caller-provided `wl_shm` `Argb8888` buffer, plus damage tracking.

use std::time::Instant;

use anyrender::ImageRenderer;

use super::{BYTES_PER_PIXEL, UiSurface};
use crate::{
    UiError,
    input::{PixelRect, RenderStats},
};

/// Frame memory kept between renders.
#[derive(Default)]
pub(super) struct FrameBuffers {
    /// Latest frame as the rasteriser wrote it (premultiplied RGBA, tightly packed).
    current: Vec<u8>,
    /// The frame before, for the damage diff.
    previous: Vec<u8>,
    has_previous: bool,
}

impl FrameBuffers {
    /// Forgets the previous frame, so the next damage is the whole buffer.
    pub(super) fn invalidate(&mut self) {
        self.has_previous = false;
    }

    fn prepare(&mut self, len: usize) {
        self.current.clear();
        self.current.resize(len, 0);
        if self.previous.len() != len {
            self.has_previous = false;
        }
    }

    fn finish_frame(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
        self.has_previous = true;
    }
}

/// Smallest buffer length that holds `height` rows of `width` pixels at `stride`.
fn required_len(width: usize, height: usize, stride: usize) -> usize {
    height
        .saturating_sub(1)
        .saturating_mul(stride)
        .saturating_add(width.saturating_mul(BYTES_PER_PIXEL))
}

/// Swaps red and blue: premultiplied RGBA bytes become little-endian `Argb8888` (B, G, R, A).
fn rgba_to_bgra(src: &[u8], dst: &mut [u8]) {
    let (from, _) = src.as_chunks::<4>();
    let (to, _) = dst.as_chunks_mut::<4>();
    for (from, to) in from.iter().zip(to) {
        *to = [from[2], from[1], from[0], from[3]];
    }
}

/// Converts a premultiplied BGRA frame (as written by [`UiSurface::render`]) into
/// straight-alpha RGBA, e.g. for PNG files. `bgra.len()` should be a multiple of 4.
pub fn bgra_premultiplied_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.as_chunks::<4>().0 {
        let alpha = u32::from(px[3]);
        let straight = |channel: u8| {
            let scaled = u32::from(channel) * 255 + alpha / 2;
            scaled.checked_div(alpha).map_or(0, |v| v.min(255)) as u8
        };
        rgba.extend_from_slice(&[straight(px[2]), straight(px[1]), straight(px[0]), px[3]]);
    }
    rgba
}

/// Bounding box of the pixels that differ between two equally sized frames.
fn damage_between(previous: &[u8], current: &[u8], width: usize) -> Option<PixelRect> {
    let row_bytes = width * BYTES_PER_PIXEL;
    let (mut min_x, mut max_x) = (usize::MAX, 0);
    let (mut min_y, mut max_y) = (usize::MAX, 0);
    let rows = previous
        .chunks_exact(row_bytes)
        .zip(current.chunks_exact(row_bytes));
    for (y, (old, new)) in rows.enumerate() {
        if old == new {
            continue;
        }
        let first = old.iter().zip(new).position(|(a, b)| a != b).unwrap_or(0);
        let last = old.iter().zip(new).rposition(|(a, b)| a != b).unwrap_or(0);
        min_x = min_x.min(first / BYTES_PER_PIXEL);
        max_x = max_x.max(last / BYTES_PER_PIXEL);
        min_y = min_y.min(y);
        max_y = y;
    }
    if min_y == usize::MAX {
        return None;
    }
    Some(PixelRect {
        x: min_x as u32,
        y: min_y as u32,
        width: (max_x - min_x + 1) as u32,
        height: (max_y - min_y + 1) as u32,
    })
}

impl UiSurface {
    /// Renders into `dst` as premultiplied `Argb8888` (the byte order wl_shm expects on little-endian).
    ///
    /// `stride` is the row pitch in bytes (at least [`min_stride`](Self::min_stride)).
    /// When [`needs_render`](Self::needs_render) is false nothing is touched and
    /// `rendered` is false. Otherwise the WHOLE buffer is written, so any buffer
    /// of a swapchain is valid afterwards; `damage` is what changed relative to
    /// the previous call, which is exactly what `wl_surface.damage_buffer` needs.
    pub fn render(&mut self, dst: &mut [u8], stride: usize) -> Result<RenderStats, UiError> {
        let (width, height) = self.buffer_size();
        let (w, h) = (width as usize, height as usize);
        let needed = required_len(w, h, stride);
        if stride < self.min_stride() || dst.len() < needed {
            return Err(UiError::BufferTooSmall {
                needed,
                stride,
                got: dst.len(),
            });
        }
        if !self.needs_render() {
            return Ok(RenderStats::default());
        }

        let started = Instant::now();
        self.ensure_layout();
        let resolved = Instant::now();
        self.frame.prepare(w * h * BYTES_PER_PIXEL);
        // The rasteriser keeps the previous frame's commands until reset; without
        // this every frame is composited over all earlier ones.
        self.renderer.reset();
        let scale = self.scale;
        let doc = &mut *self.doc;
        self.renderer.render(
            |scene| blitz_paint::paint_scene(scene, doc, scale, width, height, 0, 0),
            &mut self.frame.current,
        );
        let painted = Instant::now();

        let damage = self
            .frame
            .has_previous
            .then(|| damage_between(&self.frame.previous, &self.frame.current, w));
        let damage = damage.unwrap_or(Some(self.full_damage()));
        for (row, out) in self
            .frame
            .current
            .chunks_exact(w * BYTES_PER_PIXEL)
            .zip(dst.chunks_mut(stride))
        {
            let Some(out) = out.get_mut(..w * BYTES_PER_PIXEL) else {
                continue;
            };
            rgba_to_bgra(row, out);
        }
        self.frame.finish_frame();
        self.dirty = false;
        self.shared.clear_redraw();
        let done = Instant::now();
        Ok(RenderStats {
            rendered: true,
            damage,
            resolve: resolved - started,
            paint: painted - resolved,
            convert: done - painted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_swaps_red_and_blue_only() {
        let src = [1, 2, 3, 4, 10, 20, 30, 40];
        let mut dst = [0; 8];
        rgba_to_bgra(&src, &mut dst);
        assert_eq!(dst, [3, 2, 1, 4, 30, 20, 10, 40]);
    }

    #[test]
    fn unpremultiply_restores_straight_colours() {
        let rgba = bgra_premultiplied_to_rgba(&[0, 0, 128, 128, 1, 2, 3, 0]);
        assert_eq!(rgba, [255, 0, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn damage_is_the_bounding_box_of_changes() {
        let (w, h) = (6, 5);
        let old = vec![0u8; w * h * 4];
        let mut new = old.clone();
        assert_eq!(damage_between(&old, &new, w), None);
        new[(w + 2) * 4] = 9;
        new[(3 * w + 4) * 4 + 3] = 9;
        let rect = damage_between(&old, &new, w).unwrap();
        assert_eq!(
            rect,
            PixelRect {
                x: 2,
                y: 1,
                width: 3,
                height: 3
            }
        );
    }

    #[test]
    fn required_len_accounts_for_the_last_row() {
        assert_eq!(required_len(10, 3, 64), 2 * 64 + 40);
        assert_eq!(required_len(10, 0, 64), 40);
    }
}
