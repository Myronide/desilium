//! Animation bookkeeping for the cursor; times are passed in as milliseconds so it stays pure.

use desilium_cursor::CursorKind;

/// A wake-up closer than this would only burn CPU without a visible difference.
const MIN_WAKE_MS: u64 = 8;

/// Tracks when the current cursor kind and its current frame started to be shown.
#[derive(Debug, Default)]
pub struct AnimationClock {
    kind: Option<CursorKind>,
    started_ms: u64,
    frame: Option<usize>,
    frame_since_ms: u64,
}

impl AnimationClock {
    /// Milliseconds the animation of `kind` has been running; a new kind restarts it.
    pub fn elapsed(&mut self, kind: CursorKind, now_ms: u64) -> u64 {
        if self.kind != Some(kind) {
            *self = Self {
                kind: Some(kind),
                started_ms: now_ms,
                frame: None,
                frame_since_ms: now_ms,
            };
        }
        now_ms.saturating_sub(self.started_ms)
    }

    /// Records the frame on screen; the frame timer restarts only when it changed.
    pub fn observe_frame(&mut self, frame: usize, now_ms: u64) {
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.frame_since_ms = now_ms;
        }
    }

    /// Forgets the animation, so the next appearance starts from the first frame.
    pub fn stop(&mut self) {
        *self = Self::default();
    }

    /// Milliseconds until the current frame (shown for `delay_ms`) should be replaced.
    ///
    /// `None` for frames without a delay, which never advance.
    pub fn wake_after_ms(&self, delay_ms: u32, now_ms: u64) -> Option<u64> {
        if delay_ms == 0 {
            return None;
        }
        let shown = now_ms.saturating_sub(self.frame_since_ms);
        Some(u64::from(delay_ms).saturating_sub(shown).max(MIN_WAKE_MS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_counts_from_the_first_sighting() {
        let mut clock = AnimationClock::default();
        assert_eq!(clock.elapsed(CursorKind::Wait, 1_000), 0);
        assert_eq!(clock.elapsed(CursorKind::Wait, 1_250), 250);
    }

    #[test]
    fn changing_kind_restarts_the_animation() {
        let mut clock = AnimationClock::default();
        clock.elapsed(CursorKind::Wait, 100);
        assert_eq!(clock.elapsed(CursorKind::Progress, 500), 0);
    }

    #[test]
    fn wake_accounts_for_time_already_spent_in_the_frame() {
        let mut clock = AnimationClock::default();
        clock.elapsed(CursorKind::Wait, 0);
        clock.observe_frame(0, 0);
        assert_eq!(clock.wake_after_ms(100, 30), Some(70));
        clock.observe_frame(0, 30);
        assert_eq!(clock.wake_after_ms(100, 30), Some(70));
    }

    #[test]
    fn new_frame_restarts_the_frame_timer() {
        let mut clock = AnimationClock::default();
        clock.elapsed(CursorKind::Wait, 0);
        clock.observe_frame(0, 0);
        clock.observe_frame(1, 100);
        assert_eq!(clock.wake_after_ms(60, 110), Some(50));
    }

    #[test]
    fn overdue_frames_wake_at_the_minimum() {
        let mut clock = AnimationClock::default();
        clock.elapsed(CursorKind::Wait, 0);
        clock.observe_frame(0, 0);
        assert_eq!(clock.wake_after_ms(40, 500), Some(MIN_WAKE_MS));
    }

    #[test]
    fn static_frames_never_wake() {
        let clock = AnimationClock::default();
        assert_eq!(clock.wake_after_ms(0, 10), None);
    }

    #[test]
    fn stop_forgets_the_kind() {
        let mut clock = AnimationClock::default();
        clock.elapsed(CursorKind::Wait, 0);
        clock.stop();
        assert_eq!(clock.elapsed(CursorKind::Wait, 900), 0);
    }
}
