//! Public renderer: lays out a line, fits it to a width and rasterizes it into a premultiplied bitmap.

use fontdb::ID;

use crate::compose::Canvas;
use crate::font::FontSet;
use crate::glyph::{GlyphCache, GlyphKey, rasterize};
use crate::layout::{
    LineBox, MAX_LINE_WIDTH_PX, Truncation, build_clusters, ceil_to_px, grapheme_char_counts,
    line_box, sanitize, snap_to_px, truncate, width_limit,
};
use crate::types::{Bitmap, CacheStats, FontStack, Size, TextError, TextStyle, Weight, raster_px};

const ELLIPSIS_CHAR: char = '\u{2026}';
const ELLIPSIS_TEXT: &str = "\u{2026}";
const ASCII_ELLIPSIS_TEXT: &str = "...";

/// One char on a line; `glyph` is None for chars that no installed face can draw.
#[derive(Debug, Clone, Copy)]
struct Placed {
    glyph: Option<(ID, u16)>,
    pen_x: f32,
}

/// Laid-out glyphs and the pen position after the last one.
#[derive(Debug, Default)]
struct Run {
    glyphs: Vec<Placed>,
    end: f32,
}

/// Rasterization parameters shared by every glyph of a line.
struct Raster {
    px: f32,
    baseline: i32,
    color: [u8; 4],
}

/// CPU text rasterizer with a glyph cache. Produces premultiplied RGBA bitmaps.
pub struct TextRenderer {
    fonts: FontSet,
    cache: GlyphCache,
}

impl TextRenderer {
    /// Discovers system fonts for `stack`. Fails with [`TextError::NoFont`] when none is usable.
    pub fn new(stack: FontStack) -> Result<Self, TextError> {
        Ok(Self {
            fonts: FontSet::load(&stack)?,
            cache: GlyphCache::default(),
        })
    }

    /// Size of the untruncated line; an invalid style yields a zero size.
    pub fn measure(&mut self, text: &str, style: &TextStyle) -> Size {
        let Ok(px) = raster_px(style) else {
            return Size {
                width: 0,
                height: 0,
            };
        };
        let line = self.line_box_for(style.weight, px);
        let run = self.layout_run(&sanitize(text), style.weight, px);
        Size {
            width: ceil_to_px(run.end).min(MAX_LINE_WIDTH_PX),
            height: line.height,
        }
    }

    /// Renders one line. When it exceeds `max_width`, the tail is replaced by an ellipsis.
    pub fn render_line(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: Option<u32>,
    ) -> Result<Bitmap, TextError> {
        let px = raster_px(style)?;
        let limit = width_limit(max_width);
        let cleaned = sanitize(text);
        let run = self.layout_run(&cleaned, style.weight, px);
        let run = self.fit_run(&cleaned, style.weight, px, run, limit);
        let line = self.line_box_for(style.weight, px);
        let width = ceil_to_px(run.end).min(limit);
        let mut canvas = Canvas::new(width, line.height);
        let raster = Raster {
            px,
            baseline: line.baseline,
            color: style.color,
        };
        self.draw_run(&mut canvas, &run, &raster);
        Ok(Bitmap {
            width,
            height: line.height,
            baseline: line.baseline,
            rgba_premultiplied: canvas.pixels,
        })
    }

    /// Glyph cache counters, for verifying that caching works.
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Drops every cached glyph mask and resets the counters.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Lays out `text` on a single pen, applying per-face kerning.
    fn layout_run(&mut self, text: &str, weight: Weight, px: f32) -> Run {
        let mut run = Run::default();
        let mut previous: Option<(ID, u16)> = None;
        for ch in text.chars() {
            let Some((face, glyph)) = self.fonts.glyph_for_char(weight, ch) else {
                run.glyphs.push(Placed {
                    glyph: None,
                    pen_x: run.end,
                });
                previous = None;
                continue;
            };
            let kern = match previous {
                Some((left_face, left)) if left_face == face => {
                    self.fonts.kern(face, left, glyph, px)
                }
                _ => 0.0,
            };
            run.end += kern;
            run.glyphs.push(Placed {
                glyph: Some((face, glyph)),
                pen_x: run.end,
            });
            run.end += self.fonts.advance(face, glyph, px);
            previous = Some((face, glyph));
        }
        run
    }

    /// Returns `run` unchanged when it fits `limit`, otherwise the grapheme-cut run plus ellipsis.
    fn fit_run(&mut self, text: &str, weight: Weight, px: f32, run: Run, limit: u32) -> Run {
        let limit = limit as f32;
        if run.end <= limit {
            return run;
        }
        let ellipsis_text = self.ellipsis_text(weight);
        let ellipsis = self.layout_run(ellipsis_text, weight, px);
        let pen_x: Vec<f32> = run.glyphs.iter().map(|glyph| glyph.pen_x).collect();
        let clusters = build_clusters(&grapheme_char_counts(text), &pen_x, run.end);
        match truncate(&clusters, limit, ellipsis.end) {
            Truncation::Fits => run,
            Truncation::Cut { kept_chars } => join_with_ellipsis(run, kept_chars, ellipsis),
        }
    }

    /// The Unicode ellipsis when the stack can draw it, otherwise three ASCII dots.
    fn ellipsis_text(&mut self, weight: Weight) -> &'static str {
        if self.fonts.glyph_for_char(weight, ELLIPSIS_CHAR).is_some() {
            ELLIPSIS_TEXT
        } else {
            ASCII_ELLIPSIS_TEXT
        }
    }

    fn line_box_for(&mut self, weight: Weight, px: f32) -> LineBox {
        let (ascent, descent) = self.fonts.line_metrics(weight, px);
        line_box(ascent, descent)
    }

    fn draw_run(&mut self, canvas: &mut Canvas, run: &Run, raster: &Raster) {
        for placed in &run.glyphs {
            if let Some(face_glyph) = placed.glyph {
                self.draw_glyph(canvas, face_glyph, placed.pen_x, raster);
            }
        }
    }

    fn draw_glyph(
        &mut self,
        canvas: &mut Canvas,
        (face, index): (ID, u16),
        pen_x: f32,
        raster: &Raster,
    ) {
        let Some(font) = self.fonts.font(face) else {
            return;
        };
        let key = GlyphKey::new(face, index, raster.px);
        let Some(mask) = self
            .cache
            .get_or_insert_with(key, || rasterize(font, index, raster.px))
        else {
            return;
        };
        let origin = mask.origin(snap_to_px(pen_x), raster.baseline);
        canvas.blit(&mask.as_mask(), origin, raster.color);
    }
}

/// Keeps the first `kept_chars` of `run` and appends the ellipsis run where the cut happened.
fn join_with_ellipsis(mut run: Run, kept_chars: usize, ellipsis: Run) -> Run {
    let cut_x = run
        .glyphs
        .get(kept_chars)
        .map_or(run.end, |glyph| glyph.pen_x);
    run.glyphs.truncate(kept_chars);
    run.glyphs
        .extend(ellipsis.glyphs.into_iter().map(|glyph| Placed {
            pen_x: glyph.pen_x + cut_x,
            ..glyph
        }));
    run.end = cut_x + ellipsis.end;
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::MAX_CHARS;
    use crate::types::MAX_SCALE;

    fn renderer() -> Option<TextRenderer> {
        match TextRenderer::new(FontStack::sans()) {
            Ok(renderer) => Some(renderer),
            Err(error) => {
                eprintln!("skipping font-dependent test: {error}");
                None
            }
        }
    }

    fn style(size_px: f32, scale: f32) -> TextStyle {
        TextStyle {
            size_px,
            weight: Weight::Regular,
            color: [255, 255, 255, 255],
            scale,
        }
    }

    fn assert_premultiplied(bitmap: &Bitmap) {
        assert_eq!(
            bitmap.rgba_premultiplied.len(),
            bitmap.width as usize * bitmap.height as usize * 4
        );
        for pixel in bitmap.rgba_premultiplied.as_chunks::<4>().0.iter() {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            assert!(
                r <= a && g <= a && b <= a,
                "pixel {pixel:?} is not premultiplied"
            );
        }
    }

    #[test]
    fn empty_string_renders_zero_width_bitmap() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let bitmap = renderer
            .render_line("", &style(14.0, 1.0), None)
            .expect("render");
        assert_eq!(bitmap.width, 0);
        assert!(bitmap.rgba_premultiplied.is_empty());
        assert_eq!(renderer.measure("", &style(14.0, 1.0)).width, 0);
    }

    #[test]
    fn rendered_pixels_are_premultiplied() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let mut text_style = style(18.0, 1.0);
        text_style.color = [200, 100, 50, 180];
        let bitmap = renderer
            .render_line("Hello, World! Äöü 漢字", &text_style, None)
            .expect("render");
        assert!(bitmap.width > 0);
        assert_premultiplied(&bitmap);
        let inked = bitmap
            .rgba_premultiplied
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] > 0);
        assert!(inked, "glyphs produced no coverage");
    }

    #[test]
    fn second_render_is_served_from_cache() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        renderer.clear_cache();
        renderer
            .render_line("Desilium", &style(14.0, 1.0), None)
            .expect("first render");
        let first = renderer.cache_stats();
        renderer
            .render_line("Desilium", &style(14.0, 1.0), None)
            .expect("second render");
        let second = renderer.cache_stats();
        assert!(first.misses > 0);
        assert_eq!(second.misses, first.misses);
        assert!(second.hits > first.hits);
    }

    #[test]
    fn scale_rasterizes_at_device_pixels() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let one = renderer
            .render_line("Hello", &style(16.0, 1.0), None)
            .expect("1x");
        let two = renderer
            .render_line("Hello", &style(16.0, 2.0), None)
            .expect("2x");
        assert!(
            two.height >= one.height * 2 - 2,
            "{} vs {}",
            two.height,
            one.height
        );
        assert!(two.width as f32 > one.width as f32 * 1.8);
        assert!(two.baseline > one.baseline);
    }

    #[test]
    fn truncated_line_respects_max_width() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let text_style = style(16.0, 1.0);
        let full = renderer.measure("A considerably longer window title", &text_style);
        let max = full.width / 2;
        let bitmap = renderer
            .render_line("A considerably longer window title", &text_style, Some(max))
            .expect("render");
        assert!(bitmap.width <= max, "{} > {max}", bitmap.width);
        assert!(bitmap.width > 0);
        assert_premultiplied(&bitmap);
    }

    #[test]
    fn oversized_input_is_capped_at_4096_chars() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let text_style = style(12.0, 1.0);
        let capped = renderer.measure(&"a".repeat(MAX_CHARS), &text_style);
        let oversized = renderer.measure(&"a".repeat(10_000), &text_style);
        assert_eq!(capped, oversized);
        let bitmap = renderer
            .render_line(&"a".repeat(10_000), &text_style, None)
            .expect("render");
        assert!(bitmap.width <= MAX_LINE_WIDTH_PX);
    }

    #[test]
    fn hostile_text_renders_without_panicking() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        let text = "\u{0}\u{1b}[31m 👋🏽 مرحبا e\u{301}\u{200d}\u{feff}\u{10ffff}";
        let bitmap = renderer
            .render_line(text, &style(15.0, 1.0), Some(60))
            .expect("render");
        assert!(bitmap.width <= 60);
        assert_premultiplied(&bitmap);
    }

    #[test]
    fn invalid_style_is_rejected() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        assert!(renderer.render_line("x", &style(0.0, 1.0), None).is_err());
        assert!(
            renderer
                .render_line("x", &style(14.0, MAX_SCALE + 1.0), None)
                .is_err()
        );
        assert_eq!(
            renderer.measure("x", &style(-1.0, 1.0)),
            Size {
                width: 0,
                height: 0
            }
        );
    }

    #[test]
    fn clear_cache_resets_counters() {
        let Some(mut renderer) = renderer() else {
            return;
        };
        renderer
            .render_line("abc", &style(14.0, 1.0), None)
            .expect("render");
        renderer.clear_cache();
        assert_eq!(renderer.cache_stats(), CacheStats::default());
    }

    #[test]
    fn join_with_ellipsis_shifts_ellipsis_to_cut_point() {
        let run = Run {
            glyphs: vec![
                Placed {
                    glyph: None,
                    pen_x: 0.0,
                },
                Placed {
                    glyph: None,
                    pen_x: 4.0,
                },
                Placed {
                    glyph: None,
                    pen_x: 8.0,
                },
            ],
            end: 12.0,
        };
        let ellipsis = Run {
            glyphs: vec![Placed {
                glyph: None,
                pen_x: 0.0,
            }],
            end: 3.0,
        };
        let joined = join_with_ellipsis(run, 1, ellipsis);
        assert_eq!(joined.glyphs.len(), 2);
        assert_eq!(joined.glyphs.last().map(|g| g.pen_x), Some(4.0));
        assert_eq!(joined.end, 7.0);
    }
}
