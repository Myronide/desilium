//! Font-independent layout rules: input sanitizing, grapheme-level truncation and line box math.

use unicode_segmentation::UnicodeSegmentation;

/// Hard cap on characters laid out per call, bounding work for oversized input.
pub const MAX_CHARS: usize = 4096;
/// Widest line rendered when the caller gives no limit, in device pixels.
pub const MAX_LINE_WIDTH_PX: u32 = 8192;
/// Ascent used when a font exposes no line metrics, as a fraction of the pixel size.
const FALLBACK_ASCENT_RATIO: f32 = 0.8;
/// Descent used when a font exposes no line metrics, as a fraction of the pixel size.
const FALLBACK_DESCENT_RATIO: f32 = 0.2;

/// Replaces line breaks and tabs with spaces, drops other control characters, and caps the length.
pub fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control())
        .take(MAX_CHARS)
        .collect()
}

/// Number of chars in each extended grapheme cluster of `text`, in order.
pub fn grapheme_char_counts(text: &str) -> Vec<usize> {
    text.graphemes(true).map(|g| g.chars().count()).collect()
}

/// One grapheme cluster on a laid-out line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cluster {
    /// Number of chars the cluster spans.
    pub chars: usize,
    /// Pen advance covered by the cluster, in pixels.
    pub width: f32,
}

/// Groups per-char pen positions into grapheme clusters.
///
/// `pen_x` holds the pen position of every char; `end_x` is the pen position after the last char.
pub fn build_clusters(char_counts: &[usize], pen_x: &[f32], end_x: f32) -> Vec<Cluster> {
    let mut clusters = Vec::with_capacity(char_counts.len());
    let mut first = 0usize;
    for &count in char_counts {
        let start = pen_x.get(first).copied().unwrap_or(end_x);
        let next_first = first.saturating_add(count);
        let next = pen_x.get(next_first).copied().unwrap_or(end_x);
        clusters.push(Cluster {
            chars: count,
            width: (next - start).max(0.0),
        });
        first = next_first;
    }
    clusters
}

/// Outcome of fitting a line into a maximum width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truncation {
    /// The whole line fits unchanged.
    Fits,
    /// Keep the first `kept_chars` chars, then append the ellipsis.
    Cut {
        /// Number of leading chars that survive; always ends on a cluster boundary.
        kept_chars: usize,
    },
}

/// Decides how much of a line survives when it must fit `max_width` with a trailing ellipsis.
///
/// Whole clusters are kept or dropped, so a cut never splits a grapheme.
pub fn truncate(clusters: &[Cluster], max_width: f32, ellipsis_width: f32) -> Truncation {
    let total: f32 = clusters.iter().map(|cluster| cluster.width).sum();
    if total <= max_width {
        return Truncation::Fits;
    }
    let budget = max_width - ellipsis_width;
    let mut used = 0.0_f32;
    let mut kept_chars = 0usize;
    for cluster in clusters {
        let next = used + cluster.width;
        if next > budget {
            break;
        }
        used = next;
        kept_chars = kept_chars.saturating_add(cluster.chars);
    }
    Truncation::Cut { kept_chars }
}

/// Width limit for a line: the caller's maximum, bounded by the renderer's hard cap.
pub fn width_limit(max_width: Option<u32>) -> u32 {
    max_width.map_or(MAX_LINE_WIDTH_PX, |width| width.min(MAX_LINE_WIDTH_PX))
}

/// Vertical box of a line: where the baseline sits and how tall the bitmap is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBox {
    /// Baseline distance from the top edge, in whole pixels.
    pub baseline: i32,
    /// Total height in whole pixels.
    pub height: u32,
}

/// Builds a whole-pixel line box from font ascent and (negative) descent.
pub fn line_box(ascent: f32, descent: f32) -> LineBox {
    let ascent_px = ceil_to_px(ascent);
    let descent_px = ceil_to_px(-descent);
    LineBox {
        baseline: i32::try_from(ascent_px).unwrap_or(i32::MAX),
        height: ascent_px.saturating_add(descent_px),
    }
}

/// Ascent and descent estimated from the pixel size, for fonts without line metrics.
pub fn fallback_metrics(px: f32) -> (f32, f32) {
    (px * FALLBACK_ASCENT_RATIO, -px * FALLBACK_DESCENT_RATIO)
}

/// Rounds a non-negative measurement up to whole pixels; negative and NaN values become zero.
pub fn ceil_to_px(value: f32) -> u32 {
    value.max(0.0).ceil() as u32
}

/// Rounds a pen position to the nearest whole pixel.
pub fn snap_to_px(value: f32) -> i32 {
    value.round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_never_splits_a_grapheme_cluster() {
        // "e" + combining acute is one cluster of two chars; the cut must not split it.
        let clusters = [
            Cluster {
                chars: 1,
                width: 5.0,
            },
            Cluster {
                chars: 2,
                width: 5.0,
            },
            Cluster {
                chars: 1,
                width: 5.0,
            },
        ];
        assert_eq!(
            truncate(&clusters, 12.0, 3.0),
            Truncation::Cut { kept_chars: 1 }
        );
    }

    #[test]
    fn combining_mark_stays_with_its_base_char() {
        let counts = grapheme_char_counts("e\u{301}x");
        assert_eq!(counts, vec![2, 1]);
        let clusters = build_clusters(&counts, &[0.0, 5.0, 10.0], 15.0);
        assert_eq!(clusters[0].width, 10.0);
        assert_eq!(clusters[1].width, 5.0);
    }

    #[test]
    fn line_that_fits_is_not_cut() {
        let clusters = [Cluster {
            chars: 1,
            width: 4.0,
        }];
        assert_eq!(truncate(&clusters, 4.0, 3.0), Truncation::Fits);
    }

    #[test]
    fn empty_line_fits_even_in_zero_width() {
        assert_eq!(truncate(&[], 0.0, 3.0), Truncation::Fits);
    }

    #[test]
    fn ellipsis_wider_than_budget_keeps_nothing() {
        let clusters = [Cluster {
            chars: 1,
            width: 1.0,
        }; 4];
        assert_eq!(
            truncate(&clusters, 2.0, 3.0),
            Truncation::Cut { kept_chars: 0 }
        );
    }

    #[test]
    fn sanitize_replaces_breaks_and_drops_controls() {
        assert_eq!(sanitize("a\u{7}b\tc\n"), "ab c ");
        assert_eq!(sanitize(""), "");
    }

    #[test]
    fn sanitize_caps_length_after_dropping_controls() {
        let long = "x".repeat(10_000);
        assert_eq!(sanitize(&long).chars().count(), MAX_CHARS);
        let noisy = format!("{}y", "\u{1}".repeat(5000));
        assert_eq!(sanitize(&noisy), "y");
    }

    #[test]
    fn width_limit_applies_hard_cap() {
        assert_eq!(width_limit(None), MAX_LINE_WIDTH_PX);
        assert_eq!(width_limit(Some(10)), 10);
        assert_eq!(width_limit(Some(u32::MAX)), MAX_LINE_WIDTH_PX);
    }

    #[test]
    fn line_box_rounds_ascent_and_descent_up() {
        assert_eq!(
            line_box(12.3, -3.1),
            LineBox {
                baseline: 13,
                height: 17
            }
        );
    }

    #[test]
    fn pixel_helpers_handle_negative_and_nan() {
        assert_eq!(ceil_to_px(-1.0), 0);
        assert_eq!(ceil_to_px(f32::NAN), 0);
        assert_eq!(snap_to_px(2.5), 3);
        assert_eq!(snap_to_px(-0.4), 0);
    }
}
