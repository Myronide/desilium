//! Window button glyph textures (cached per kind, size, stroke and colour).

use crate::decoration::{GlyphKind, rasterize_glyph};
use desilium_theme::Color;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

/// Everything that changes the pixels of a glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub kind: GlyphKind,
    pub edge_px: u32,
    pub stroke_bits: u32,
    pub color: [u8; 4],
}

/// Builds the premultiplied texture of a glyph.
pub fn render(key: &GlyphKey) -> Option<MemoryRenderBuffer> {
    let coverage = rasterize_glyph(key.kind, key.edge_px, f32::from_bits(key.stroke_bits));
    let [red, green, blue, alpha] = key.color;
    let mut pixels = Vec::with_capacity(coverage.len() * 4);
    for value in coverage {
        let ink = u32::from(alpha) * u32::from(value);
        let premultiply =
            |channel: u8| ((u32::from(channel) * ink + 127 * 255) / (255 * 255)) as u8;
        pixels.extend_from_slice(&[
            premultiply(red),
            premultiply(green),
            premultiply(blue),
            ((ink + 127) / 255) as u8,
        ]);
    }
    let edge = i32::try_from(key.edge_px).ok()?;
    Some(MemoryRenderBuffer::from_slice(
        &pixels,
        Fourcc::Abgr8888,
        (edge, edge),
        1,
        Transform::Normal,
        None,
    ))
}

/// 8-bit straight colour of a theme colour.
pub fn bytes(color: Color) -> [u8; 4] {
    color.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_textures_build_for_every_kind() {
        for kind in [
            GlyphKind::Minimize,
            GlyphKind::Maximize,
            GlyphKind::Restore,
            GlyphKind::Close,
        ] {
            let key = GlyphKey {
                kind,
                edge_px: 20,
                stroke_bits: 1.9_f32.to_bits(),
                color: [200, 210, 220, 255],
            };
            assert!(render(&key).is_some());
        }
    }
}
