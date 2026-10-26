//! Validation and normalisation of the `image-data` hint into RGBA pixels.

/// Largest accepted edge length in pixels.
pub const IMAGE_MAX_EDGE: usize = 512;
/// Largest accepted raw payload; also bounds the decoded RGBA size.
pub const IMAGE_MAX_BYTES: usize = IMAGE_MAX_EDGE * IMAGE_MAX_EDGE * 4;

/// The `(iiibiiay)` structure as sent by clients, before validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub channels: i32,
    pub data: Vec<u8>,
}

/// Tightly packed RGBA8 pixels, row by row, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Reasons an `image-data` hint is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("image payload exceeds {IMAGE_MAX_BYTES} bytes")]
    Oversize,
    #[error("image dimensions are outside 1..={IMAGE_MAX_EDGE}")]
    Dimensions,
    #[error("only 8-bit RGB or RGBA images are supported")]
    Format,
    #[error("image payload is shorter than its dimensions require")]
    ShortData,
}

/// Validates a raw image and repacks it as tightly packed RGBA8.
pub fn to_rgba(raw: RawImage) -> Result<Rgba, ImageError> {
    if raw.data.len() > IMAGE_MAX_BYTES {
        return Err(ImageError::Oversize);
    }
    let width = edge(raw.width)?;
    let height = edge(raw.height)?;
    let channels = channel_count(&raw)?;
    let row_bytes = width.checked_mul(channels).ok_or(ImageError::Dimensions)?;
    let stride = usize::try_from(raw.rowstride).map_err(|_| ImageError::Format)?;
    if stride < row_bytes {
        return Err(ImageError::Format);
    }
    let needed = stride
        .checked_mul(height - 1)
        .and_then(|prefix| prefix.checked_add(row_bytes))
        .ok_or(ImageError::ShortData)?;
    if raw.data.len() < needed {
        return Err(ImageError::ShortData);
    }
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = row * stride;
        let line = raw
            .data
            .get(start..start + row_bytes)
            .ok_or(ImageError::ShortData)?;
        for pixel in line.chunks_exact(channels) {
            pixels.extend_from_slice(&rgba_of(pixel));
        }
    }
    Ok(Rgba {
        width: u32::try_from(width).map_err(|_| ImageError::Dimensions)?,
        height: u32::try_from(height).map_err(|_| ImageError::Dimensions)?,
        pixels,
    })
}

fn edge(value: i32) -> Result<usize, ImageError> {
    match usize::try_from(value) {
        Ok(edge) if (1..=IMAGE_MAX_EDGE).contains(&edge) => Ok(edge),
        _ => Err(ImageError::Dimensions),
    }
}

fn channel_count(raw: &RawImage) -> Result<usize, ImageError> {
    if raw.bits_per_sample != 8 {
        return Err(ImageError::Format);
    }
    match (raw.channels, raw.has_alpha) {
        (4, true) => Ok(4),
        (3, false) => Ok(3),
        _ => Err(ImageError::Format),
    }
}

fn rgba_of(pixel: &[u8]) -> [u8; 4] {
    match *pixel {
        [r, g, b, a] => [r, g, b, a],
        [r, g, b] => [r, g, b, u8::MAX],
        _ => [0, 0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(width: i32, height: i32, rowstride: i32, channels: i32, data: Vec<u8>) -> RawImage {
        RawImage {
            width,
            height,
            rowstride,
            has_alpha: channels == 4,
            bits_per_sample: 8,
            channels,
            data,
        }
    }

    #[test]
    fn rgba_passes_through_unchanged() {
        let img = to_rgba(raw(2, 1, 8, 4, vec![1, 2, 3, 4, 5, 6, 7, 8])).expect("valid");
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn rgb_gets_opaque_alpha() {
        let img = to_rgba(raw(1, 1, 3, 3, vec![9, 8, 7])).expect("valid");
        assert_eq!(img.pixels, vec![9, 8, 7, 255]);
    }

    #[test]
    fn padded_rows_are_repacked() {
        // Two pixels per row, stride padded to 12 bytes.
        let data = vec![1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 3, 3, 3, 3, 4, 4, 4, 4];
        let img = to_rgba(raw(2, 2, 12, 4, data)).expect("valid");
        assert_eq!(
            img.pixels,
            vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4]
        );
    }

    #[test]
    fn zero_or_oversized_dimensions_are_rejected() {
        assert_eq!(
            to_rgba(raw(0, 1, 4, 4, vec![0; 4])),
            Err(ImageError::Dimensions)
        );
        assert_eq!(
            to_rgba(raw(-1, 1, 4, 4, vec![0; 4])),
            Err(ImageError::Dimensions)
        );
        let wide = raw(513, 1, 513 * 4, 4, vec![0; 513 * 4]);
        assert_eq!(to_rgba(wide), Err(ImageError::Dimensions));
    }

    #[test]
    fn oversized_payload_is_rejected_before_inspection() {
        let huge = raw(1, 1, 4, 4, vec![0; IMAGE_MAX_BYTES + 1]);
        assert_eq!(to_rgba(huge), Err(ImageError::Oversize));
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        let mut sixteen = raw(1, 1, 4, 4, vec![0; 4]);
        sixteen.bits_per_sample = 16;
        assert_eq!(to_rgba(sixteen), Err(ImageError::Format));
        let mut mismatch = raw(1, 1, 4, 4, vec![0; 4]);
        mismatch.has_alpha = false;
        assert_eq!(to_rgba(mismatch), Err(ImageError::Format));
        assert_eq!(
            to_rgba(raw(1, 1, 2, 4, vec![0; 4])),
            Err(ImageError::Format)
        );
    }

    #[test]
    fn short_payload_is_rejected() {
        assert_eq!(
            to_rgba(raw(2, 2, 8, 4, vec![0; 12])),
            Err(ImageError::ShortData)
        );
        assert_eq!(
            to_rgba(raw(1, 2, i32::MAX, 4, vec![0; 4])),
            Err(ImageError::ShortData)
        );
    }

    #[test]
    fn negative_rowstride_is_rejected() {
        assert_eq!(
            to_rgba(raw(1, 1, -4, 4, vec![0; 4])),
            Err(ImageError::Format)
        );
    }
}
