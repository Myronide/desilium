//! Title text bitmaps, rasterized on the CPU and uploaded as premultiplied textures.

use desilium_text::{TextRenderer, TextStyle, Weight};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

const BYTES_PER_PIXEL: usize = 4;

/// Everything that changes the pixels of a title.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TitleKey {
    pub text: String,
    /// Widest the bitmap may get in device pixels; longer titles end in an ellipsis.
    pub width_px: u32,
    pub scale_bits: u32,
    pub size_bits: u32,
    pub color: [u8; 4],
    pub weight: u16,
}

/// A rasterized title ready to be drawn.
pub struct TitleImage {
    pub buffer: MemoryRenderBuffer,
    /// Size in whole logical pixels such that scaling it by the output scale gives the bitmap size.
    pub logical: (i32, i32),
    /// Bitmap size in device pixels.
    pub pixels: (i32, i32),
    /// Distance from the top of the bitmap to the baseline in device pixels.
    pub baseline: i32,
}

/// Maps a CSS weight onto the classes the text renderer knows.
pub fn weight_class(css: u16) -> Weight {
    match css {
        0..=450 => Weight::Regular,
        451..=600 => Weight::Medium,
        _ => Weight::Bold,
    }
}

/// Rasterizes `key`; `None` for empty text or an unusable style.
pub fn render(renderer: &mut TextRenderer, key: &TitleKey) -> Option<TitleImage> {
    let scale = f32::from_bits(key.scale_bits);
    let style = TextStyle {
        size_px: f32::from_bits(key.size_bits),
        weight: weight_class(key.weight),
        color: key.color,
        scale,
    };
    let bitmap = renderer
        .render_line(&key.text, &style, Some(key.width_px))
        .ok()?;
    if bitmap.width == 0 || bitmap.height == 0 || key.text.trim().is_empty() {
        return None;
    }
    let (padded_w, logical_w) = snap_to_logical(bitmap.width, scale);
    let (padded_h, logical_h) = snap_to_logical(bitmap.height, scale);
    let pixels = pad(&bitmap.rgba_premultiplied, bitmap.width, padded_w, padded_h);
    let size = (i32::try_from(padded_w).ok()?, i32::try_from(padded_h).ok()?);
    Some(TitleImage {
        buffer: MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Abgr8888,
            size,
            1,
            Transform::Normal,
            None,
        ),
        logical: (logical_w, logical_h),
        pixels: size,
        baseline: bitmap.baseline,
    })
}

/// Smallest whole logical length whose device size is at least `device`, and that device size.
///
/// The compositor sizes textures in whole logical pixels, so padding the bitmap to exactly
/// `round(logical * scale)` keeps the glyphs unscaled and sharp.
fn snap_to_logical(device: u32, scale: f32) -> (u32, i32) {
    let logical = (device as f32 / scale).ceil().max(1.0);
    let padded = (logical * scale).round().max(device as f32);
    (padded as u32, logical as i32)
}

fn pad(source: &[u8], width: u32, padded_w: u32, padded_h: u32) -> Vec<u8> {
    let stride = width as usize * BYTES_PER_PIXEL;
    let padded_stride = padded_w as usize * BYTES_PER_PIXEL;
    let mut out = vec![0; padded_stride * padded_h as usize];
    for (row, line) in source.chunks_exact(stride).enumerate() {
        let start = row * padded_stride;
        if let Some(target) = out.get_mut(start..start + stride) {
            target.copy_from_slice(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_never_shrinks_and_matches_the_logical_size() {
        for scale in [1.0_f32, 1.25, 1.5, 2.0, 3.0] {
            for device in 1..200_u32 {
                let (padded, logical) = snap_to_logical(device, scale);
                assert!(padded >= device, "{device} {scale}");
                assert_eq!(
                    (logical as f32 * scale).round() as u32,
                    padded,
                    "{device} {scale}"
                );
            }
        }
    }

    #[test]
    fn padding_keeps_rows_and_zero_fills() {
        let source = [1_u8; 2 * 2 * 4];
        let out = pad(&source, 2, 3, 3);
        assert_eq!(out.len(), 3 * 3 * 4);
        assert_eq!(&out[0..8], &[1; 8]);
        assert_eq!(&out[8..12], &[0; 4]);
        assert!(out[24..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn weights_map_to_classes() {
        assert_eq!(weight_class(400), Weight::Regular);
        assert_eq!(weight_class(500), Weight::Medium);
        assert_eq!(weight_class(600), Weight::Medium);
        assert_eq!(weight_class(700), Weight::Bold);
    }

    #[test]
    fn rendering_without_fonts_or_text_is_none() {
        let Ok(mut renderer) = TextRenderer::new(desilium_text::FontStack::sans()) else {
            return;
        };
        let key = TitleKey {
            text: "  ".into(),
            width_px: 100,
            scale_bits: 1.0_f32.to_bits(),
            size_bits: 13.0_f32.to_bits(),
            color: [255; 4],
            weight: 500,
        };
        assert!(render(&mut renderer, &key).is_none());
        let key = TitleKey {
            text: "Terminal".into(),
            ..key
        };
        let image = render(&mut renderer, &key);
        assert!(image.is_some_and(|image| image.logical.0 > 0));
    }
}
