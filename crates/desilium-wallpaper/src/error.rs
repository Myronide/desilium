//! Typed errors for wallpaper loading and construction.

use std::path::PathBuf;

use crate::geometry::Size;

/// Errors returned while loading or constructing a wallpaper.
#[derive(Debug, thiserror::Error)]
pub enum WallpaperError {
    /// The file could not be opened or its metadata could not be read.
    #[error("cannot read wallpaper {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file content is not a supported image format.
    #[error("{path} is not a supported image format")]
    UnknownFormat { path: PathBuf },
    /// The file is larger than the accepted limit, so it was rejected without decoding.
    #[error("{path} is {bytes} bytes, above the {limit} byte limit")]
    FileTooLarge {
        path: PathBuf,
        bytes: u64,
        limit: u64,
    },
    /// Width or height exceeds the accepted edge length, so it was rejected without decoding.
    #[error("image is {}x{} pixels, above the {max_edge}px edge limit", .size.w, .size.h)]
    OversizedDimensions { size: Size, max_edge: u32 },
    /// The image has a zero width or height.
    #[error("image has no pixels")]
    EmptyImage,
    /// The image decoder rejected the data (corrupt, truncated or over its allocation limit).
    #[error("cannot decode {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
    /// A raw pixel buffer does not match its declared size.
    #[error("pixel buffer holds {actual} bytes, expected {expected}")]
    BufferSizeMismatch { expected: usize, actual: usize },
    /// The pixel buffer size for these dimensions cannot be represented.
    #[error("image of {}x{} pixels is too large to address", .size.w, .size.h)]
    SizeOverflow { size: Size },
}
