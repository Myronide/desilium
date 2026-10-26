//! Parses XCursor binary files into decoded cursor sets.

use std::collections::BTreeMap;

use xcursor::parser::{Image, parse_xcursor};

use crate::CursorImage;
use crate::frames::{LoadedCursor, NominalSet};
use crate::image::argb_le_to_rgba_premultiplied;

const BYTES_PER_PIXEL: usize = 4;

/// Decodes an XCursor file; `None` if the bytes are not a usable cursor.
pub(crate) fn parse_cursor_file(bytes: &[u8]) -> Option<LoadedCursor> {
    let images = parse_xcursor(bytes)?;
    let mut by_size: BTreeMap<u32, Vec<CursorImage>> = BTreeMap::new();
    for image in &images {
        if let Some(frame) = decode_image(image) {
            by_size.entry(image.size).or_default().push(frame);
        }
    }
    let sets = by_size
        .into_iter()
        .map(|(size, frames)| NominalSet::new(size, frames))
        .collect();
    LoadedCursor::new(sets)
}

/// The crate's `pixels_rgba` field is the untouched file payload: little-endian ARGB words.
fn decode_image(image: &Image) -> Option<CursorImage> {
    let expected = (image.width as usize)
        .checked_mul(image.height as usize)?
        .checked_mul(BYTES_PER_PIXEL)?;
    if image.pixels_rgba.len() != expected {
        return None;
    }
    Some(CursorImage {
        width: image.width,
        height: image.height,
        hotspot_x: image.xhot,
        hotspot_y: image.yhot,
        rgba_premultiplied: argb_le_to_rgba_premultiplied(&image.pixels_rgba),
        delay_ms: image.delay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{encode_cursor_file, frame};

    #[test]
    fn decodes_argb_little_endian_pixels_into_premultiplied_rgba() {
        // A = 0x80, R = 0xff: half-transparent red.
        let bytes = encode_cursor_file(&[frame(8, 0, (1, 2), 0x80ff_0000)]);
        let cursor = parse_cursor_file(&bytes).expect("valid file");
        let image = cursor
            .nearest(8)
            .and_then(|set| set.frame_at(0))
            .expect("frame present");
        assert_eq!(&image.rgba_premultiplied[..4], &[128, 0, 0, 128]);
        assert_eq!((image.hotspot_x, image.hotspot_y), (1, 2));
    }

    #[test]
    fn groups_frames_by_nominal_size() {
        let bytes = encode_cursor_file(&[
            frame(24, 10, (0, 0), 0xff00_0000),
            frame(24, 20, (0, 0), 0xff00_0000),
            frame(32, 0, (0, 0), 0xff00_0000),
        ]);
        let cursor = parse_cursor_file(&bytes).expect("valid file");
        assert!(cursor.is_animated());
        let small = cursor.nearest(24).expect("24 present");
        assert_eq!(small.frame_count(), 2);
        assert_eq!(small.frame_at(0).map(|frame| frame.width), Some(24));
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(parse_cursor_file(b"definitely not an xcursor file").is_none());
        assert!(parse_cursor_file(&[]).is_none());
    }

    #[test]
    fn every_decoded_pixel_is_premultiplied() {
        let bytes = encode_cursor_file(&[frame(8, 0, (0, 0), 0x40ff_8040)]);
        let cursor = parse_cursor_file(&bytes).expect("valid file");
        let image = cursor
            .nearest(8)
            .and_then(|set| set.frame_at(0))
            .expect("frame");
        for px in image.rgba_premultiplied.as_chunks::<4>().0 {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }
}
