//! Frame type handed to the compositor and the ARGB to premultiplied RGBA conversion.

const BYTES_PER_PIXEL: usize = 4;
const CHANNEL_MAX: u32 = 255;
const ROUNDING: u32 = CHANNEL_MAX / 2;

/// One cursor frame: premultiplied RGBA8, row-major, origin at the top-left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    /// Pixel inside the image that sits under the pointer position.
    pub hotspot_x: u32,
    /// Row of the hotspot inside the image.
    pub hotspot_y: u32,
    /// `width * height * 4` bytes; every colour channel is at most the alpha.
    pub rgba_premultiplied: Vec<u8>,
    /// How long this frame is shown; 0 for static cursors.
    pub delay_ms: u32,
}

/// Converts one XCursor pixel (`0xAARRGGBB`) into premultiplied RGBA bytes.
pub fn premultiply_argb(argb: u32) -> [u8; 4] {
    let alpha = (argb >> 24) & 0xff;
    let red = (argb >> 16) & 0xff;
    let green = (argb >> 8) & 0xff;
    let blue = argb & 0xff;
    [
        premultiply_channel(red, alpha),
        premultiply_channel(green, alpha),
        premultiply_channel(blue, alpha),
        alpha as u8,
    ]
}

/// Multiplies one channel by alpha with rounding to the nearest value.
fn premultiply_channel(channel: u32, alpha: u32) -> u8 {
    ((channel * alpha + ROUNDING) / CHANNEL_MAX) as u8
}

/// Converts a little-endian ARGB byte stream (the layout inside XCursor files) to premultiplied RGBA.
pub(crate) fn argb_le_to_rgba_premultiplied(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    for chunk in raw.as_chunks::<BYTES_PER_PIXEL>().0 {
        out.extend_from_slice(&premultiply_argb(u32::from_le_bytes(*chunk)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_pixel_keeps_colour() {
        assert_eq!(premultiply_argb(0xff_10_20_30), [0x10, 0x20, 0x30, 0xff]);
    }

    #[test]
    fn half_alpha_pixel_is_premultiplied() {
        assert_eq!(premultiply_argb(0x80_ff_00_00), [128, 0, 0, 128]);
    }

    #[test]
    fn transparent_pixel_is_fully_zero() {
        assert_eq!(premultiply_argb(0x00_ff_ff_ff), [0, 0, 0, 0]);
    }

    #[test]
    fn rounding_is_to_nearest() {
        // 255 * 1 / 255 = 1 exactly, 128 * 255 / 255 = 128 with alpha 255.
        assert_eq!(premultiply_argb(0x01_ff_ff_ff), [1, 1, 1, 1]);
        assert_eq!(premultiply_argb(0xff_80_00_00)[0], 128);
    }

    #[test]
    fn little_endian_byte_stream_is_decoded_as_argb_words() {
        // Word 0xff102030 is stored little-endian as 30 20 10 ff.
        let raw = [0x30, 0x20, 0x10, 0xff];
        assert_eq!(
            argb_le_to_rgba_premultiplied(&raw),
            vec![0x10, 0x20, 0x30, 0xff]
        );
    }

    #[test]
    fn trailing_partial_pixel_is_ignored() {
        let raw = [0x30, 0x20, 0x10, 0xff, 0x01];
        assert_eq!(argb_le_to_rgba_premultiplied(&raw).len(), 4);
    }
}
