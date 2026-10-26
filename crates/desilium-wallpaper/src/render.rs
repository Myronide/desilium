//! Composes a [`Wallpaper`] onto an output-sized buffer over a background color.

use crate::color::{CHANNEL_MAX, to_byte};
use crate::geometry::{BYTES_PER_PIXEL, FitMode, RectI, Size, fit_placement};
use crate::load::Wallpaper;
use crate::resample::{build_taps, resample};

impl Wallpaper {
    /// Renders the wallpaper onto a tightly packed RGBA8 buffer of `output` size.
    ///
    /// `background` shows through letterbox bars and transparent pixels. Returns an empty buffer when
    /// `output` is empty or its byte size does not fit in `usize`.
    pub fn render(&self, output: Size, mode: FitMode, background: [u8; 4]) -> Vec<u8> {
        let Some(len) = output.rgba_len() else {
            return Vec::new();
        };
        if len == 0 {
            return Vec::new();
        }
        let mut canvas: Vec<u8> = background.iter().cycle().take(len).copied().collect();
        if mode == FitMode::Tile {
            self.draw_tiles(&mut canvas, output);
        } else {
            self.draw_scaled(&mut canvas, output, mode);
        }
        canvas
    }

    fn draw_scaled(&self, canvas: &mut [u8], output: Size, mode: FitMode) {
        let placement = fit_placement(self.size, output, mode);
        let dst = placement.dst;
        if dst.w == 0 || dst.h == 0 {
            return;
        }
        let src = placement.src;
        let (width, height) = (self.size.w as usize, self.size.h as usize);
        let x_taps = build_taps(width, src.x, src.w, dst.w as usize);
        let y_taps = build_taps(height, src.y, src.h, dst.h as usize);
        let scaled = resample(&self.pixels, width, height, &x_taps, &y_taps);
        blit_over(canvas, output.w as usize, dst, &scaled);
    }

    fn draw_tiles(&self, canvas: &mut [u8], output: Size) {
        let row_len = output.w as usize * BYTES_PER_PIXEL;
        for (y, row) in canvas.chunks_exact_mut(row_len.max(1)).enumerate() {
            for (x, pixel) in row
                .as_chunks_mut::<BYTES_PER_PIXEL>()
                .0
                .iter_mut()
                .enumerate()
            {
                let Some(source) = self.tiled_pixel(x, y) else {
                    continue;
                };
                let blended = over(source, to_array(pixel));
                pixel.copy_from_slice(&blended);
            }
        }
    }

    /// Source pixel for an output position, repeating the image in both axes.
    fn tiled_pixel(&self, x: usize, y: usize) -> Option<[u8; BYTES_PER_PIXEL]> {
        let width = self.size.w as usize;
        let sx = x.checked_rem(width)?;
        let sy = y.checked_rem(self.size.h as usize)?;
        let offset = (sy * width + sx) * BYTES_PER_PIXEL;
        let bytes = self.pixels.get(offset..offset + BYTES_PER_PIXEL)?;
        <[u8; BYTES_PER_PIXEL]>::try_from(bytes).ok()
    }
}

/// Copies the scaled tile into the canvas at `dst`, compositing over what is already there.
fn blit_over(canvas: &mut [u8], canvas_width: usize, dst: RectI, tile: &[u8]) {
    let row_len = canvas_width * BYTES_PER_PIXEL;
    let tile_row_len = dst.w as usize * BYTES_PER_PIXEL;
    let x_offset = usize::try_from(dst.x).unwrap_or(0) * BYTES_PER_PIXEL;
    let y_offset = usize::try_from(dst.y).unwrap_or(0);
    let rows = canvas
        .chunks_exact_mut(row_len.max(1))
        .skip(y_offset)
        .take(dst.h as usize);
    for (canvas_row, tile_row) in rows.zip(tile.chunks_exact(tile_row_len.max(1))) {
        let Some(target) = canvas_row.get_mut(x_offset..x_offset + tile_row_len) else {
            continue;
        };
        for (pixel, source) in target
            .as_chunks_mut::<BYTES_PER_PIXEL>()
            .0
            .iter_mut()
            .zip(tile_row.as_chunks::<BYTES_PER_PIXEL>().0.iter())
        {
            let blended = over(to_array(source), to_array(pixel));
            pixel.copy_from_slice(&blended);
        }
    }
}

/// Porter-Duff "over" for straight-alpha RGBA8.
fn over(front: [u8; BYTES_PER_PIXEL], back: [u8; BYTES_PER_PIXEL]) -> [u8; BYTES_PER_PIXEL] {
    let [fr, fg, fb, fa] = front;
    if fa == u8::MAX {
        return front;
    }
    let front_alpha = f32::from(fa) / CHANNEL_MAX;
    let back_alpha = f32::from(back[3]) / CHANNEL_MAX;
    let out_alpha = front_alpha + back_alpha * (1.0 - front_alpha);
    if out_alpha <= 0.0 {
        return [0; BYTES_PER_PIXEL];
    }
    let mix = |front_channel: u8, back_channel: u8| {
        let weighted = f32::from(front_channel) * front_alpha
            + f32::from(back_channel) * back_alpha * (1.0 - front_alpha);
        to_byte(weighted / out_alpha)
    };
    let [br, bg, bb, _] = back;
    [
        mix(fr, br),
        mix(fg, bg),
        mix(fb, bb),
        to_byte(out_alpha * CHANNEL_MAX),
    ]
}

/// Reads one pixel, falling back to transparent black for a short slice.
fn to_array(pixel: &[u8]) -> [u8; BYTES_PER_PIXEL] {
    <[u8; BYTES_PER_PIXEL]>::try_from(pixel).unwrap_or([0; BYTES_PER_PIXEL])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: Size, rgba: [u8; 4]) -> Wallpaper {
        let len = size.rgba_len().expect("len");
        let pixels = rgba.iter().cycle().take(len).copied().collect();
        Wallpaper::from_rgba(size, pixels).expect("valid")
    }

    fn pixel(buffer: &[u8], size: Size, x: usize, y: usize) -> [u8; 4] {
        let offset = (y * size.w as usize + x) * 4;
        to_array(&buffer[offset..offset + 4])
    }

    #[test]
    fn output_buffer_has_exact_length() {
        let wallpaper = solid(Size::new(4, 4), [1, 2, 3, 255]);
        let out = Size::new(10, 7);
        for mode in [
            FitMode::Fill,
            FitMode::Fit,
            FitMode::Stretch,
            FitMode::Center,
            FitMode::Tile,
        ] {
            assert_eq!(
                wallpaper.render(out, mode, [0; 4]).len(),
                10 * 7 * 4,
                "{mode:?}"
            );
        }
    }

    #[test]
    fn stretch_of_solid_color_is_that_color_everywhere() {
        let wallpaper = solid(Size::new(3, 5), [200, 10, 90, 255]);
        let out = Size::new(17, 9);
        let buffer = wallpaper.render(out, FitMode::Stretch, [0, 0, 0, 255]);
        assert!(
            buffer
                .as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [200, 10, 90, 255])
        );
    }

    #[test]
    fn fit_letterboxes_with_background() {
        let wallpaper = solid(Size::new(20, 10), [255, 255, 255, 255]);
        let out = Size::new(20, 20);
        let buffer = wallpaper.render(out, FitMode::Fit, [7, 7, 7, 255]);
        assert_eq!(pixel(&buffer, out, 10, 0), [7, 7, 7, 255]);
        assert_eq!(pixel(&buffer, out, 10, 10), [255, 255, 255, 255]);
    }

    #[test]
    fn tile_repeats_source_at_native_scale() {
        let mut pixels = Vec::new();
        for value in [10u8, 20] {
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
        let wallpaper = Wallpaper::from_rgba(Size::new(2, 1), pixels).expect("valid");
        let out = Size::new(5, 1);
        let buffer = wallpaper.render(out, FitMode::Tile, [0; 4]);
        let values: Vec<u8> = buffer.as_chunks::<4>().0.iter().map(|px| px[0]).collect();
        assert_eq!(values, [10, 20, 10, 20, 10]);
    }

    #[test]
    fn empty_output_yields_empty_buffer() {
        let wallpaper = solid(Size::new(2, 2), [0, 0, 0, 255]);
        assert!(
            wallpaper
                .render(Size::new(0, 5), FitMode::Fill, [0; 4])
                .is_empty()
        );
        assert!(
            wallpaper
                .render(Size::new(u32::MAX, u32::MAX), FitMode::Fill, [0; 4])
                .is_empty()
        );
    }

    #[test]
    fn transparent_pixels_show_the_background() {
        let wallpaper = solid(Size::new(2, 2), [255, 0, 0, 0]);
        let out = Size::new(2, 2);
        let buffer = wallpaper.render(out, FitMode::Stretch, [0, 0, 255, 255]);
        assert!(
            buffer
                .as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [0, 0, 255, 255])
        );
    }

    #[test]
    fn over_keeps_opaque_front_and_blends_partial_alpha() {
        assert_eq!(over([1, 2, 3, 255], [9, 9, 9, 255]), [1, 2, 3, 255]);
        let half = over([255, 255, 255, 128], [0, 0, 0, 255]);
        assert!((i16::from(half[0]) - 128).abs() <= 2, "{half:?}");
        assert_eq!(half[3], 255);
    }
}
