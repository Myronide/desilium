//! Wallpaper decoding. Size limits are checked before any pixel data is decoded.

use std::fs;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::error::WallpaperError;
use crate::geometry::{FitMode, Size, fit_placement};

/// Largest file accepted for decoding.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Largest accepted width or height in pixels.
pub const MAX_EDGE_PX: u32 = 16_384;
/// Cap on the decoder's own allocations; a maximal RGBA image needs 1 GiB.
const MAX_DECODE_ALLOC_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const TRANSPARENT: [u8; 4] = [0; 4];

/// Options for [`Wallpaper::load_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadOptions {
    /// Images larger than this (aspect ratio kept) are downscaled right after decoding.
    pub max_size: Option<Size>,
}

/// A decoded wallpaper as tightly packed, straight-alpha RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wallpaper {
    pub(crate) size: Size,
    pub(crate) pixels: Vec<u8>,
}

impl Wallpaper {
    /// Loads and decodes an image file with the default options.
    pub fn load(path: &Path) -> Result<Wallpaper, WallpaperError> {
        Self::load_with(path, &LoadOptions::default())
    }

    /// Loads and decodes an image file, applying the given options.
    pub fn load_with(path: &Path, options: &LoadOptions) -> Result<Wallpaper, WallpaperError> {
        check_file_size(path)?;
        let decoder = open_decoder(path)?;
        let (w, h) = decoder.dimensions();
        let size = Size::new(w, h);
        check_edges(size)?;
        let image =
            DynamicImage::from_decoder(decoder).map_err(|source| decode_error(path, source))?;
        let wallpaper = Wallpaper::from_rgba(size, image.into_rgba8().into_raw())?;
        Ok(match options.max_size {
            Some(max) => downscale_if_larger(wallpaper, max),
            None => wallpaper,
        })
    }

    /// Wraps an already decoded, tightly packed RGBA8 buffer.
    pub fn from_rgba(size: Size, pixels: Vec<u8>) -> Result<Wallpaper, WallpaperError> {
        if size.is_empty() {
            return Err(WallpaperError::EmptyImage);
        }
        let expected = size
            .rgba_len()
            .ok_or(WallpaperError::SizeOverflow { size })?;
        if pixels.len() != expected {
            return Err(WallpaperError::BufferSizeMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Wallpaper { size, pixels })
    }

    /// Pixel dimensions of the wallpaper.
    pub fn dimensions(&self) -> Size {
        self.size
    }

    /// Tightly packed straight-alpha RGBA8 pixels, row by row from the top.
    pub fn rgba(&self) -> &[u8] {
        &self.pixels
    }
}

/// Downscales the wallpaper so that it fits within `max`, keeping its aspect ratio.
///
/// Images already within `max` (or any image when `max` is empty) are returned unchanged.
pub fn downscale_if_larger(wallpaper: Wallpaper, max: Size) -> Wallpaper {
    let fits = wallpaper.size.w <= max.w && wallpaper.size.h <= max.h;
    if max.is_empty() || fits {
        return wallpaper;
    }
    let target = fit_placement(wallpaper.size, max, FitMode::Fit).dst;
    let size = Size::new(target.w, target.h);
    let pixels = wallpaper.render(size, FitMode::Stretch, TRANSPARENT);
    Wallpaper { size, pixels }
}

fn check_file_size(path: &Path) -> Result<(), WallpaperError> {
    let bytes = fs::metadata(path)
        .map_err(|source| io_error(path, source))?
        .len();
    if bytes > MAX_FILE_BYTES {
        return Err(WallpaperError::FileTooLarge {
            path: path.to_path_buf(),
            bytes,
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

fn open_decoder(path: &Path) -> Result<impl ImageDecoder, WallpaperError> {
    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|source| io_error(path, source))?;
    if reader.format().is_none() {
        return Err(WallpaperError::UnknownFormat {
            path: path.to_path_buf(),
        });
    }
    reader.limits(decode_limits());
    reader
        .into_decoder()
        .map_err(|source| decode_error(path, source))
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    limits
}

fn check_edges(size: Size) -> Result<(), WallpaperError> {
    if size.is_empty() {
        return Err(WallpaperError::EmptyImage);
    }
    if size.w > MAX_EDGE_PX || size.h > MAX_EDGE_PX {
        return Err(WallpaperError::OversizedDimensions {
            size,
            max_edge: MAX_EDGE_PX,
        });
    }
    Ok(())
}

fn io_error(path: &Path, source: std::io::Error) -> WallpaperError {
    WallpaperError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn decode_error(path: &Path, source: image::ImageError) -> WallpaperError {
    WallpaperError::Decode {
        path: PathBuf::from(path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

    fn write_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        let image = RgbaImage::from_fn(w, h, |x, y| Rgba([x as u8, y as u8, 128, 255]));
        image
            .save_with_format(&path, ImageFormat::Png)
            .expect("write png fixture");
        path
    }

    #[test]
    fn loads_png_with_expected_dimensions_and_pixels() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_png(dir.path(), "a.png", 4, 3);
        let wallpaper = Wallpaper::load(&path).expect("load png");
        assert_eq!(wallpaper.dimensions(), Size::new(4, 3));
        assert_eq!(wallpaper.rgba()[..4], [0, 0, 128, 255]);
        assert_eq!(wallpaper.rgba().len(), 4 * 3 * 4);
    }

    #[test]
    fn loads_jpeg() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("b.jpg");
        let image = RgbImage::from_fn(40, 20, |x, y| Rgb([x as u8, y as u8, 0]));
        image
            .save_with_format(&path, ImageFormat::Jpeg)
            .expect("write jpeg fixture");
        let wallpaper = Wallpaper::load(&path).expect("load jpeg");
        assert_eq!(wallpaper.dimensions(), Size::new(40, 20));
    }

    #[test]
    fn rejects_unknown_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        // No recognised extension either, so neither magic bytes nor the file name identify a format.
        let path = dir.path().join("junk.dat");
        fs::write(&path, b"definitely not an image at all").expect("write");
        let err = Wallpaper::load(&path).expect_err("garbage must fail");
        assert!(
            matches!(err, WallpaperError::UnknownFormat { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rejects_truncated_png_without_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let full = write_png(dir.path(), "full.png", 64, 64);
        let bytes = fs::read(&full).expect("read");
        let truncated = dir.path().join("truncated.png");
        fs::write(&truncated, &bytes[..bytes.len() / 2]).expect("write");
        assert!(Wallpaper::load(&truncated).is_err());
    }

    #[test]
    fn rejects_oversized_edge_before_decoding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_png(dir.path(), "wide.png", MAX_EDGE_PX + 1, 1);
        let err = Wallpaper::load(&path).expect_err("too wide");
        assert!(
            matches!(err, WallpaperError::OversizedDimensions { max_edge, .. } if max_edge == MAX_EDGE_PX),
            "{err:?}"
        );
    }

    #[test]
    fn rejects_files_above_size_limit_without_reading_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.png");
        let file = fs::File::create(&path).expect("create");
        file.set_len(MAX_FILE_BYTES + 1).expect("sparse file");
        let err = Wallpaper::load(&path).expect_err("too large");
        assert!(
            matches!(err, WallpaperError::FileTooLarge { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn missing_file_is_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = Wallpaper::load(&dir.path().join("nope.png")).expect_err("missing");
        assert!(matches!(err, WallpaperError::Io { .. }), "{err:?}");
    }

    #[test]
    fn load_with_max_size_downscales_preserving_aspect() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_png(dir.path(), "big.png", 64, 32);
        let options = LoadOptions {
            max_size: Some(Size::new(16, 16)),
        };
        let wallpaper = Wallpaper::load_with(&path, &options).expect("load");
        assert_eq!(wallpaper.dimensions(), Size::new(16, 8));
        assert_eq!(wallpaper.rgba().len(), 16 * 8 * 4);
    }

    #[test]
    fn downscale_if_larger_keeps_small_images() {
        let small = Wallpaper::from_rgba(Size::new(8, 8), vec![9; 8 * 8 * 4]).expect("valid");
        let unchanged = downscale_if_larger(small.clone(), Size::new(16, 16));
        assert_eq!(unchanged, small);
    }

    #[test]
    fn from_rgba_rejects_wrong_buffer_length() {
        let err = Wallpaper::from_rgba(Size::new(2, 2), vec![0; 15]).expect_err("short");
        assert!(
            matches!(
                err,
                WallpaperError::BufferSizeMismatch {
                    expected: 16,
                    actual: 15
                }
            ),
            "{err:?}"
        );
    }
}
