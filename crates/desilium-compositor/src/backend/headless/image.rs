use crate::error::CompositorError;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

const CHANNELS: usize = 4;
const ALPHA_OPAQUE: u32 = 255;

/// A CPU-side RGBA8 picture with straight (non-premultiplied) alpha, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// Builds an image from premultiplied pixels, optionally flipping rows vertically.
    pub fn from_premultiplied(width: u32, height: u32, mut rgba: Vec<u8>, flip: bool) -> Self {
        if flip {
            flip_rows(&mut rgba, (width as usize).saturating_mul(CHANNELS));
        }
        for pixel in rgba.as_chunks_mut::<CHANNELS>().0 {
            unpremultiply(pixel);
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    /// The RGBA bytes of one pixel, or `None` when outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(CHANNELS)?;
        let bytes = self.rgba.get(index..index.checked_add(CHANNELS)?)?;
        bytes.try_into().ok()
    }

    /// Writes the image as an 8-bit RGBA PNG.
    pub fn save_png(&self, path: &Path) -> Result<(), CompositorError> {
        let file = File::create(path).map_err(screenshot_error)?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(screenshot_error)?;
        writer
            .write_image_data(&self.rgba)
            .map_err(screenshot_error)?;
        writer.finish().map_err(screenshot_error)
    }
}

fn screenshot_error(error: impl std::fmt::Display) -> CompositorError {
    CompositorError::Screenshot(error.to_string())
}

fn flip_rows(data: &mut [u8], stride: usize) {
    if stride == 0 {
        return;
    }
    let rows = data.len() / stride;
    for row in 0..rows / 2 {
        let (top, bottom) = data.split_at_mut((rows - 1 - row).saturating_mul(stride));
        let start = row.saturating_mul(stride);
        if let (Some(a), Some(b)) = (top.get_mut(start..start + stride), bottom.get_mut(..stride)) {
            a.swap_with_slice(b);
        }
    }
}

fn unpremultiply(pixel: &mut [u8; CHANNELS]) {
    let [red, green, blue, alpha] = pixel;
    let alpha_value = u32::from(*alpha);
    if alpha_value == 0 || alpha_value == ALPHA_OPAQUE {
        return;
    }
    for channel in [red, green, blue] {
        let scaled = u32::from(*channel) * ALPHA_OPAQUE / alpha_value;
        *channel = u8::try_from(scaled.min(ALPHA_OPAQUE)).unwrap_or(u8::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_rows() {
        let data = vec![1, 1, 1, 255, 2, 2, 2, 255];
        let image = Image::from_premultiplied(1, 2, data, true);
        assert_eq!(image.pixel(0, 0), Some([2, 2, 2, 255]));
        assert_eq!(image.pixel(0, 1), Some([1, 1, 1, 255]));
    }

    #[test]
    fn unpremultiplies_half_alpha() {
        let image = Image::from_premultiplied(1, 1, vec![64, 32, 0, 128], false);
        assert_eq!(image.pixel(0, 0), Some([127, 63, 0, 128]));
    }

    #[test]
    fn pixel_out_of_range_is_none() {
        let image = Image::from_premultiplied(1, 1, vec![0, 0, 0, 255], false);
        assert_eq!(image.pixel(1, 0), None);
    }

    #[test]
    fn saves_a_decodable_png() {
        let image = Image::from_premultiplied(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255], false);
        let path = std::env::temp_dir().join(format!("desilium-img-{}.png", std::process::id()));
        image.save_png(&path).unwrap();
        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buffer).unwrap();
        assert_eq!(&buffer[..8], image.rgba.as_slice());
        std::fs::remove_file(path).unwrap();
    }
}
