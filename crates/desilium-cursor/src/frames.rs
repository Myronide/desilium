//! Decoded cursor sets grouped by nominal size, and animation frame selection.

use std::cmp::Reverse;

use crate::CursorImage;

/// All frames of one nominal size, in file order. Animated cursors have several.
pub(crate) struct NominalSet {
    size: u32,
    frames: Vec<CursorImage>,
}

/// Every nominal size a cursor file provides, sorted ascending by size.
pub(crate) struct LoadedCursor {
    sets: Vec<NominalSet>,
}

impl NominalSet {
    pub(crate) fn new(size: u32, frames: Vec<CursorImage>) -> NominalSet {
        NominalSet { size, frames }
    }

    /// Frame to show `elapsed_ms` after the animation started; loops over the total duration.
    pub(crate) fn frame_at(&self, elapsed_ms: u64) -> Option<&CursorImage> {
        let index = frame_index(self.frames.iter().map(|frame| frame.delay_ms), elapsed_ms);
        self.frames.get(index)
    }

    fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    #[cfg(test)]
    pub(crate) fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

impl LoadedCursor {
    /// `None` when there is no nominal size at all.
    pub(crate) fn new(sets: Vec<NominalSet>) -> Option<LoadedCursor> {
        (!sets.is_empty()).then_some(LoadedCursor { sets })
    }

    /// Set whose nominal size is closest to `target`; ties prefer the larger size.
    pub(crate) fn nearest(&self, target: u32) -> Option<&NominalSet> {
        self.sets
            .iter()
            .min_by_key(|set| (set.size.abs_diff(target), Reverse(set.size)))
    }

    pub(crate) fn is_animated(&self) -> bool {
        self.sets.iter().any(NominalSet::is_animated)
    }
}

/// Index of the frame active at `elapsed_ms` for the given per-frame delays.
///
/// Zero-delay frames are skipped; when all delays are zero the first frame is used.
pub(crate) fn frame_index(delays: impl Iterator<Item = u32> + Clone, elapsed_ms: u64) -> usize {
    let total: u64 = delays.clone().map(u64::from).sum();
    if total == 0 {
        return 0;
    }
    let mut remaining = elapsed_ms % total;
    let mut last = 0;
    for (index, delay) in delays.enumerate() {
        let delay = u64::from(delay);
        if remaining < delay {
            return index;
        }
        remaining = remaining.saturating_sub(delay);
        last = index;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_at(delays: &[u32], elapsed_ms: u64) -> usize {
        frame_index(delays.iter().copied(), elapsed_ms)
    }

    #[test]
    fn selects_frames_by_elapsed_time() {
        let delays = [10, 20, 30];
        assert_eq!(index_at(&delays, 0), 0);
        assert_eq!(index_at(&delays, 9), 0);
        assert_eq!(index_at(&delays, 10), 1);
        assert_eq!(index_at(&delays, 29), 1);
        assert_eq!(index_at(&delays, 30), 2);
        assert_eq!(index_at(&delays, 59), 2);
    }

    #[test]
    fn wraps_around_total_duration() {
        let delays = [10, 20, 30];
        assert_eq!(index_at(&delays, 60), 0);
        assert_eq!(index_at(&delays, 75), 1);
        assert_eq!(index_at(&delays, 6_000_000_000 + 30), 2);
    }

    #[test]
    fn skips_zero_delay_frames() {
        let delays = [0, 5];
        assert_eq!(index_at(&delays, 0), 1);
        assert_eq!(index_at(&delays, 4), 1);
        assert_eq!(index_at(&delays, 5), 1);
    }

    #[test]
    fn all_zero_delays_show_first_frame() {
        assert_eq!(index_at(&[0, 0], 1234), 0);
        assert_eq!(index_at(&[], 1234), 0);
    }

    #[test]
    fn nearest_prefers_closest_then_larger_size() {
        let set = |size| NominalSet::new(size, Vec::new());
        let cursor = LoadedCursor::new(vec![set(24), set(32), set(48)]).expect("non-empty");
        assert_eq!(cursor.nearest(30).map(|s| s.size), Some(32));
        assert_eq!(cursor.nearest(28).map(|s| s.size), Some(32));
        assert_eq!(cursor.nearest(100).map(|s| s.size), Some(48));
    }

    #[test]
    fn empty_cursor_is_rejected() {
        assert!(LoadedCursor::new(Vec::new()).is_none());
    }
}
