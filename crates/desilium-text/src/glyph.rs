//! Glyph rasterization and the cache of gamma-adjusted coverage masks.

use std::collections::HashMap;

use fontdb::ID;
use fontdue::Font;

use crate::compose::{Mask, apply_lut, coverage_lut};
use crate::types::CacheStats;

/// Entry count at which the cache is flushed, bounding memory for long sessions.
pub const MAX_CACHED_GLYPHS: usize = 8192;
/// Sub-pixel resolution of the size component of cache keys.
const SIZE_KEY_STEPS_PER_PX: f32 = 64.0;

/// Identifies one rasterized glyph: face, glyph index and quantized pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    face: ID,
    glyph: u16,
    size_steps: u32,
}

impl GlyphKey {
    /// Key for `glyph` of `face` rasterized at `px` pixels.
    pub fn new(face: ID, glyph: u16, px: f32) -> Self {
        Self {
            face,
            glyph,
            size_steps: (px * SIZE_KEY_STEPS_PER_PX).round() as u32,
        }
    }
}

/// Coverage of one glyph with its placement relative to the pen.
pub struct GlyphMask {
    /// Horizontal offset of the mask's left edge from the pen.
    left: i32,
    /// Vertical offset of the mask's bottom edge above the baseline (y grows upward).
    bottom: i32,
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

impl GlyphMask {
    /// View of the coverage for compositing.
    pub fn as_mask(&self) -> Mask<'_> {
        Mask {
            width: self.width,
            height: self.height,
            coverage: &self.coverage,
        }
    }

    /// Top-left corner of the mask for a pen at `pen_x` on `baseline` (y grows downward).
    pub fn origin(&self, pen_x: i32, baseline: i32) -> (i32, i32) {
        let height = i32::try_from(self.height).unwrap_or(i32::MAX);
        let top = baseline.saturating_sub(self.bottom).saturating_sub(height);
        (pen_x.saturating_add(self.left), top)
    }
}

/// Rasterizes one glyph and applies the coverage gamma.
pub fn rasterize(font: &Font, glyph: u16, px: f32) -> GlyphMask {
    let (metrics, coverage) = font.rasterize_indexed(glyph, px);
    let lut = coverage_lut();
    GlyphMask {
        left: metrics.xmin,
        bottom: metrics.ymin,
        width: metrics.width,
        height: metrics.height,
        coverage: coverage.into_iter().map(|c| apply_lut(&lut, c)).collect(),
    }
}

/// Glyph mask cache with hit and miss counters.
#[derive(Default)]
pub struct GlyphCache {
    masks: HashMap<GlyphKey, GlyphMask>,
    hits: u64,
    misses: u64,
}

impl GlyphCache {
    /// Returns the cached mask, calling `make` only on a miss.
    pub fn get_or_insert_with(
        &mut self,
        key: GlyphKey,
        make: impl FnOnce() -> GlyphMask,
    ) -> Option<&GlyphMask> {
        if self.masks.contains_key(&key) {
            self.hits = self.hits.saturating_add(1);
        } else {
            self.misses = self.misses.saturating_add(1);
            if self.masks.len() >= MAX_CACHED_GLYPHS {
                self.masks.clear();
            }
            self.masks.insert(key, make());
        }
        self.masks.get(&key)
    }

    /// Current counters and entry count.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.masks.len(),
        }
    }

    /// Drops all masks and resets the counters.
    pub fn clear(&mut self) {
        self.masks.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(width: usize, height: usize) -> GlyphMask {
        GlyphMask {
            left: 0,
            bottom: 0,
            width,
            height,
            coverage: vec![0; width * height],
        }
    }

    fn face_id() -> ID {
        ID::dummy()
    }

    #[test]
    fn second_lookup_of_same_key_is_a_hit() {
        let mut cache = GlyphCache::default();
        let key = GlyphKey::new(face_id(), 7, 16.0);
        assert!(cache.get_or_insert_with(key, || mask(2, 2)).is_some());
        assert!(cache.get_or_insert_with(key, || mask(9, 9)).is_some());
        assert_eq!(
            cache.stats(),
            CacheStats {
                hits: 1,
                misses: 1,
                entries: 1
            }
        );
    }

    #[test]
    fn different_size_is_a_different_entry() {
        let mut cache = GlyphCache::default();
        cache.get_or_insert_with(GlyphKey::new(face_id(), 7, 16.0), || mask(2, 2));
        cache.get_or_insert_with(GlyphKey::new(face_id(), 7, 32.0), || mask(4, 4));
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.stats().entries, 2);
    }

    #[test]
    fn clear_resets_entries_and_counters() {
        let mut cache = GlyphCache::default();
        cache.get_or_insert_with(GlyphKey::new(face_id(), 1, 10.0), || mask(1, 1));
        cache.clear();
        assert_eq!(cache.stats(), CacheStats::default());
    }

    #[test]
    fn origin_places_mask_above_baseline_and_offsets_left_edge() {
        let glyph = GlyphMask {
            left: 2,
            bottom: -1,
            width: 4,
            height: 10,
            coverage: Vec::new(),
        };
        assert_eq!(glyph.origin(5, 12), (7, 3));
    }
}
