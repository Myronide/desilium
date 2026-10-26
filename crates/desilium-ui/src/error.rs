//! Error type of the UI host.

/// Everything that can go wrong in `desilium-ui`.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    /// The selector is outside the supported subset.
    #[error("unsupported selector `{0}` (use #id, .class, [data-key] or [data-key=value])")]
    InvalidSelector(String),
    /// No element matches the selector.
    #[error("no element matches `{0}`")]
    NoMatch(String),
    /// The requested surface size cannot be rendered.
    #[error("invalid surface size {width}x{height} at scale {scale}")]
    InvalidSize {
        /// Logical width in pixels.
        width: u32,
        /// Logical height in pixels.
        height: u32,
        /// Scale factor.
        scale: f64,
    },
    /// The destination buffer cannot hold a frame.
    #[error("destination buffer too small: need {needed} bytes (stride {stride}), got {got}")]
    BufferTooSmall {
        /// Bytes required.
        needed: usize,
        /// Stride that was passed in.
        stride: usize,
        /// Bytes available.
        got: usize,
    },
    /// A font could not be registered.
    #[error("font data is not a usable font")]
    BadFont,
    /// Wayland connection or protocol problem.
    #[cfg(feature = "wayland")]
    #[error("wayland: {0}")]
    Wayland(String),
}
