//! Anti-aliased coverage masks of the window button glyphs (CPU, resolution independent).
//!
//! Shapes are defined as fractions of the glyph box so they stay proportional at any scale, and
//! straight edges snap to the pixel grid so thin strokes stay crisp at 1x, 1.25x, 1.5x and 2x.

/// Which symbol to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphKind {
    Minimize,
    Maximize,
    /// Two overlapping squares, shown on a maximized window.
    Restore,
    Close,
}

/// Reference box the proportions below are written against (the theme's default glyph size).
const REFERENCE_BOX: f32 = 16.0;
const LINE_HALF: f32 = 5.0 / REFERENCE_BOX;
const CROSS_HALF: f32 = 4.5 / REFERENCE_BOX;
const SQUARE: (f32, f32) = (3.5 / REFERENCE_BOX, 12.5 / REFERENCE_BOX);
const SQUARE_RADIUS: f32 = 2.0 / REFERENCE_BOX;
const FRONT: ((f32, f32), (f32, f32)) = (
    (3.5 / REFERENCE_BOX, 10.5 / REFERENCE_BOX),
    (5.5 / REFERENCE_BOX, 12.5 / REFERENCE_BOX),
);
const BACK: ((f32, f32), (f32, f32)) = (
    (5.5 / REFERENCE_BOX, 12.5 / REFERENCE_BOX),
    (3.5 / REFERENCE_BOX, 10.5 / REFERENCE_BOX),
);
const RESTORE_RADIUS: f32 = 1.5 / REFERENCE_BOX;
/// Gap between the front square and the part of the back square that tucks behind it.
const RESTORE_GAP: f32 = 1.25 / REFERENCE_BOX;
/// Strokes thinner than this many pixels always centre on a pixel row.
const THIN_STROKE_PX: f32 = 2.0;

#[derive(Clone, Copy)]
struct Pen {
    /// Edge of the square bitmap in pixels.
    edge: f32,
    stroke: f32,
}

impl Pen {
    fn at(self, fraction: f32) -> f32 {
        fraction * self.edge
    }

    /// Moves a stroke centreline onto the grid so it covers whole pixels symmetrically.
    fn snap(self, position: f32) -> f32 {
        let odd = (self.stroke.round() as i64) % 2 != 0;
        if self.stroke < THIN_STROKE_PX || odd {
            return position.floor() + 0.5;
        }
        position.round()
    }
}

/// Coverage mask (`edge_px * edge_px` bytes, row-major, 255 = full ink) of `kind`.
///
/// `stroke_px` is the stroke width in device pixels.
pub fn rasterize(kind: GlyphKind, edge_px: u32, stroke_px: f32) -> Vec<u8> {
    let pen = Pen {
        edge: edge_px as f32,
        stroke: stroke_px.max(0.0),
    };
    let mut mask = Vec::with_capacity((edge_px as usize).saturating_mul(edge_px as usize));
    for row in 0..edge_px {
        for column in 0..edge_px {
            let point = (column as f32 + 0.5, row as f32 + 0.5);
            let coverage = coverage_at(kind, pen, point).clamp(0.0, 1.0);
            mask.push((coverage * 255.0).round() as u8);
        }
    }
    mask
}

fn coverage_at(kind: GlyphKind, pen: Pen, point: (f32, f32)) -> f32 {
    match kind {
        GlyphKind::Minimize => minimize(pen, point),
        GlyphKind::Maximize => outline(pen, point, (SQUARE, SQUARE), SQUARE_RADIUS),
        GlyphKind::Restore => restore(pen, point),
        GlyphKind::Close => close(pen, point),
    }
}

/// Pixel coverage of a signed distance (negative inside), one pixel wide ramp.
fn ramp(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn minimize(pen: Pen, point: (f32, f32)) -> f32 {
    let middle = pen.snap(pen.at(0.5));
    let left = pen.snap(pen.at(0.5 - LINE_HALF));
    let right = pen.snap(pen.at(0.5 + LINE_HALF));
    let along = (point.0 - left.max(point.0).min(right)).abs();
    let across = (point.1 - middle).abs();
    ramp((along * along + across * across).sqrt() - pen.stroke / 2.0)
}

fn close(pen: Pen, point: (f32, f32)) -> f32 {
    let (low, high) = (pen.at(0.5 - CROSS_HALF), pen.at(0.5 + CROSS_HALF));
    let down = segment_distance(point, (low, low), (high, high));
    let up = segment_distance(point, (low, high), (high, low));
    ramp(down.min(up) - pen.stroke / 2.0)
}

fn segment_distance(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq > 0.0 {
        (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (nearest_x, nearest_y) = (from.0 + t * dx, from.1 + t * dy);
    ((point.0 - nearest_x).powi(2) + (point.1 - nearest_y).powi(2)).sqrt()
}

/// Signed distance to a rounded box given by snapped centrelines.
fn box_distance(pen: Pen, point: (f32, f32), spans: ((f32, f32), (f32, f32)), radius: f32) -> f32 {
    let (x, y) = spans;
    let (left, right) = (pen.snap(pen.at(x.0)), pen.snap(pen.at(x.1)));
    let (top, bottom) = (pen.snap(pen.at(y.0)), pen.snap(pen.at(y.1)));
    let radius = pen
        .at(radius)
        .min((right - left) / 2.0)
        .min((bottom - top) / 2.0);
    let half = ((right - left) / 2.0, (bottom - top) / 2.0);
    let local = (
        (point.0 - (left + right) / 2.0).abs() - (half.0 - radius),
        (point.1 - (top + bottom) / 2.0).abs() - (half.1 - radius),
    );
    let outside = (local.0.max(0.0).powi(2) + local.1.max(0.0).powi(2)).sqrt();
    outside + local.0.max(local.1).min(0.0) - radius
}

fn outline(pen: Pen, point: (f32, f32), spans: ((f32, f32), (f32, f32)), radius: f32) -> f32 {
    ramp(box_distance(pen, point, spans, radius).abs() - pen.stroke / 2.0)
}

fn restore(pen: Pen, point: (f32, f32)) -> f32 {
    let front = outline(pen, point, FRONT, RESTORE_RADIUS);
    let back = outline(pen, point, BACK, RESTORE_RADIUS);
    let reach = box_distance(pen, point, FRONT, RESTORE_RADIUS) - pen.at(RESTORE_GAP);
    let tucked = ramp(reach);
    front.max(back * (1.0 - tucked))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [GlyphKind; 4] = [
        GlyphKind::Minimize,
        GlyphKind::Maximize,
        GlyphKind::Restore,
        GlyphKind::Close,
    ];

    fn at(mask: &[u8], edge: u32, column: u32, row: u32) -> u8 {
        mask[(row * edge + column) as usize]
    }

    #[test]
    fn masks_have_the_requested_size() {
        for kind in KINDS {
            assert_eq!(rasterize(kind, 20, 1.9).len(), 400);
        }
    }

    #[test]
    fn every_glyph_has_ink_and_leaves_the_corners_empty() {
        for kind in KINDS {
            for (edge, stroke) in [(16, 1.5), (20, 1.9), (24, 2.25), (32, 3.0)] {
                let mask = rasterize(kind, edge, stroke);
                assert!(mask.iter().any(|value| *value > 200), "{kind:?} {edge}");
                assert_eq!(at(&mask, edge, 0, 0), 0, "{kind:?} {edge}");
                assert_eq!(at(&mask, edge, edge - 1, edge - 1), 0, "{kind:?} {edge}");
            }
        }
    }

    #[test]
    fn minimize_is_a_thin_horizontal_bar() {
        let mask = rasterize(GlyphKind::Minimize, 16, 1.5);
        let rows: Vec<u32> = (0..16)
            .filter(|row| (0..16).any(|column| at(&mask, 16, column, *row) > 0))
            .collect();
        assert!(rows.len() <= 3, "{rows:?}");
        let full = (0..16)
            .filter(|column| at(&mask, 16, *column, rows[1]) == 255)
            .count();
        assert!(full >= 8, "{full}");
    }

    #[test]
    fn single_pixel_strokes_snap_to_a_full_row() {
        let mask = rasterize(GlyphKind::Minimize, 16, 1.0);
        let lit: Vec<u32> = (0..16).filter(|row| at(&mask, 16, 8, *row) > 0).collect();
        assert_eq!(lit.len(), 1);
        assert_eq!(at(&mask, 16, 8, lit[0]), 255);
    }

    #[test]
    fn close_is_symmetric() {
        let mask = rasterize(GlyphKind::Close, 20, 1.9);
        for row in 0..20 {
            for column in 0..20 {
                let mirrored = at(&mask, 20, 19 - column, row);
                let value = at(&mask, 20, column, row);
                assert!(value.abs_diff(mirrored) <= 2, "({column},{row})");
            }
        }
    }

    #[test]
    fn maximize_is_hollow_and_restore_differs() {
        let maximize = rasterize(GlyphKind::Maximize, 16, 1.5);
        assert_eq!(at(&maximize, 16, 8, 8), 0);
        assert!(at(&maximize, 16, 3, 8) > 100 || at(&maximize, 16, 4, 8) > 100);
        assert_ne!(maximize, rasterize(GlyphKind::Restore, 16, 1.5));
    }

    #[test]
    fn zero_sized_requests_do_not_panic() {
        assert!(rasterize(GlyphKind::Close, 0, 1.5).is_empty());
        assert_eq!(rasterize(GlyphKind::Close, 4, f32::NAN).len(), 16);
    }
}
