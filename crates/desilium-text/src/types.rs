//! Public value types shared by the layout, cache and renderer modules.

/// Largest scale factor accepted for HiDPI rasterization.
pub const MAX_SCALE: f32 = 8.0;
/// Largest rasterized pixel size (`size_px * scale`); bounds bitmap memory per line.
pub const MAX_RASTER_PX: f32 = 1024.0;

/// Ordered list of font family names; the first installed family providing a glyph wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontStack {
    /// Family names in preference order, e.g. `["Inter", "Noto Sans"]`.
    pub families: Vec<String>,
}

impl FontStack {
    /// Default UI stack used by the compositor when no configuration is given.
    pub fn sans() -> Self {
        Self {
            families: ["Inter", "Noto Sans", "Cantarell", "DejaVu Sans"]
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }
}

/// Font weight class; maps onto CSS numeric weights 400, 500 and 700.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    /// CSS weight 400.
    Regular,
    /// CSS weight 500.
    Medium,
    /// CSS weight 700.
    Bold,
}

impl Weight {
    /// Numeric CSS weight used for font matching.
    pub fn css_value(self) -> u16 {
        match self {
            Self::Regular => 400,
            Self::Medium => 500,
            Self::Bold => 700,
        }
    }
}

/// Appearance of one line of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Logical font size in pixels, before `scale` is applied.
    pub size_px: f32,
    /// Weight class used to pick faces from the stack.
    pub weight: Weight,
    /// Straight (non-premultiplied) RGBA color.
    pub color: [u8; 4],
    /// HiDPI factor; glyphs are rasterized at `size_px * scale` device pixels.
    pub scale: f32,
}

/// A rendered line in device pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels (line box, ascent plus descent).
    pub height: u32,
    /// Distance from the top edge to the text baseline, in device pixels.
    pub baseline: i32,
    /// Row-major, tightly packed premultiplied RGBA, `width * height * 4` bytes.
    pub rgba_premultiplied: Vec<u8>,
}

/// Size of a laid-out line in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Advance width in device pixels.
    pub width: u32,
    /// Line box height in device pixels.
    pub height: u32,
}

/// Glyph cache counters, for verifying that caching works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    /// Glyph lookups served from the cache.
    pub hits: u64,
    /// Glyph lookups that had to rasterize.
    pub misses: u64,
    /// Glyph masks currently held.
    pub entries: usize,
}

/// Errors returned by the text renderer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    /// The system has no font that can be loaded for the stack.
    #[error("no usable system font found")]
    NoFont,
    /// The style cannot be rasterized; the payload names the violated rule.
    #[error("invalid text style: {0}")]
    InvalidStyle(&'static str),
}

/// Validates a style and returns the pixel size to rasterize at.
pub fn raster_px(style: &TextStyle) -> Result<f32, TextError> {
    if !style.size_px.is_finite() || style.size_px <= 0.0 {
        return Err(TextError::InvalidStyle(
            "size_px must be finite and positive",
        ));
    }
    if !style.scale.is_finite() || style.scale <= 0.0 || style.scale > MAX_SCALE {
        return Err(TextError::InvalidStyle("scale must be in (0, 8]"));
    }
    let px = style.size_px * style.scale;
    if px > MAX_RASTER_PX {
        return Err(TextError::InvalidStyle("size_px * scale exceeds 1024"));
    }
    Ok(px)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size_px: f32, scale: f32) -> TextStyle {
        TextStyle {
            size_px,
            weight: Weight::Regular,
            color: [0, 0, 0, 255],
            scale,
        }
    }

    #[test]
    fn raster_px_multiplies_size_by_scale() {
        assert_eq!(raster_px(&style(14.0, 2.0)), Ok(28.0));
    }

    #[test]
    fn raster_px_rejects_non_positive_and_non_finite_sizes() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(raster_px(&style(bad, 1.0)).is_err(), "size {bad}");
        }
    }

    #[test]
    fn raster_px_rejects_bad_scale_and_oversized_raster() {
        assert!(raster_px(&style(14.0, 0.0)).is_err());
        assert!(raster_px(&style(14.0, 9.0)).is_err());
        assert!(raster_px(&style(300.0, 4.0)).is_err());
        assert!(raster_px(&style(256.0, 4.0)).is_ok());
    }

    #[test]
    fn sans_stack_lists_default_families_in_order() {
        let stack = FontStack::sans();
        assert_eq!(stack.families.first().map(String::as_str), Some("Inter"));
        assert_eq!(stack.families.len(), 4);
    }
}
