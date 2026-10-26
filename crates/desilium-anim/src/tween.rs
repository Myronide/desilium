//! A single value that moves from one endpoint to another over a fixed time span.

use std::time::Duration;

use crate::easing::Easing;
use crate::lerp::Lerp;

/// Interpolates from `from` to `to` between `start` and `start + duration`.
///
/// Before `start` the value holds at `from`; from the end onwards it holds at `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween<T: Lerp> {
    from: T,
    to: T,
    start: Duration,
    duration: Duration,
    easing: Easing,
}

impl<T: Lerp> Tween<T> {
    /// Creates a tween that runs from `from` to `to` starting at `start`.
    pub fn new(from: T, to: T, start: Duration, duration: Duration, easing: Easing) -> Self {
        Self {
            from,
            to,
            start,
            duration,
            easing,
        }
    }

    /// Returns the interpolated value at time `now`.
    pub fn value(&self, now: Duration) -> T {
        if now < self.start {
            return self.from;
        }
        if self.is_finished(now) {
            return self.to;
        }
        // Here `now` is in `start..end` with a positive duration, so the ratio is in `0..1`.
        let elapsed = now.saturating_sub(self.start);
        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased = self.easing.apply(progress);
        if eased <= 0.0 {
            return self.from;
        }
        self.from.lerp(self.to, eased)
    }

    /// Returns `true` once `now` has reached the end time. Zero-length tweens are finished
    /// as soon as `now` reaches their start.
    pub fn is_finished(&self, now: Duration) -> bool {
        now >= self.end_time()
    }

    /// Returns `true` while the tween is actively changing the value at `now`.
    pub fn is_running(&self, now: Duration) -> bool {
        now >= self.start && now < self.end_time()
    }

    /// Returns the value this tween ends on.
    pub fn target(&self) -> T {
        self.to
    }

    /// Returns the time at which the tween begins.
    pub fn start_time(&self) -> Duration {
        self.start
    }

    /// Returns the time at which the tween ends.
    pub fn end_time(&self) -> Duration {
        self.start.saturating_add(self.duration)
    }

    /// Starts a new tween toward `new_to` from the value this tween has at `now`.
    ///
    /// Because the new tween begins at the current value, redirecting an animation
    /// mid-flight never makes the value jump.
    pub fn retarget(
        &self,
        now: Duration,
        new_to: T,
        duration: Duration,
        easing: Easing,
    ) -> Tween<T> {
        Tween::new(self.value(now), new_to, now, duration, easing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn linear(from: f32, to: f32) -> Tween<f32> {
        Tween::new(from, to, ms(1000), ms(200), Easing::Linear)
    }

    #[test]
    fn holds_start_value_before_start() {
        let tween = linear(0.0, 10.0);
        assert_eq!(tween.value(ms(0)), 0.0);
        assert_eq!(tween.value(ms(999)), 0.0);
        assert!(!tween.is_finished(ms(999)));
        assert!(!tween.is_running(ms(999)));
    }

    #[test]
    fn interpolates_linearly_while_running() {
        let tween = linear(0.0, 10.0);
        assert!((tween.value(ms(1100)) - 5.0).abs() < 1e-4);
        assert!(tween.is_running(ms(1100)));
        assert!(!tween.is_finished(ms(1100)));
    }

    #[test]
    fn holds_target_value_after_end() {
        let tween = linear(0.0, 10.0);
        assert_eq!(tween.value(ms(1200)), 10.0);
        assert_eq!(tween.value(ms(5000)), 10.0);
        assert!(tween.is_finished(ms(1200)));
        assert!(!tween.is_running(ms(1200)));
        assert_eq!(tween.target(), 10.0);
    }

    #[test]
    fn zero_duration_finishes_immediately() {
        let tween = Tween::new(1.0_f32, 2.0, ms(500), Duration::ZERO, Easing::EaseOutExpo);
        assert!(tween.is_finished(ms(500)));
        assert!(!tween.is_running(ms(500)));
        assert_eq!(tween.value(ms(500)), 2.0);
    }

    #[test]
    fn eased_value_follows_easing_curve() {
        let tween = Tween::new(0.0_f32, 1.0, ms(0), ms(100), Easing::EaseOutQuad);
        let expected = Easing::EaseOutQuad.apply(0.25);
        assert!((tween.value(ms(25)) - expected).abs() < 1e-5);
    }

    #[test]
    fn works_for_tuples() {
        let tween = Tween::new(
            (0.0_f32, 0.0_f32),
            (4.0, 8.0),
            ms(0),
            ms(100),
            Easing::Linear,
        );
        assert_eq!(tween.value(ms(50)), (2.0, 4.0));
    }

    #[test]
    fn retarget_starts_from_current_value() {
        let tween = linear(0.0, 10.0);
        let now = ms(1100);
        let current = tween.value(now);
        let retargeted = tween.retarget(now, 100.0, ms(300), Easing::Linear);
        assert_eq!(retargeted.value(now), current);
        assert_eq!(retargeted.target(), 100.0);
        assert!(retargeted.is_finished(now + ms(300)));
        assert_eq!(retargeted.value(now + ms(300)), 100.0);
    }

    #[test]
    fn retarget_is_continuous_across_many_redirects() {
        let mut tween = Tween::new(0.0_f32, 1.0, ms(0), ms(500), Easing::EaseOutCubic);
        let mut now = ms(0);
        for step in 1..=20_u64 {
            now += ms(10);
            let before = tween.value(now);
            let next_target = if step % 2 == 0 { 0.0 } else { 1.0 };
            tween = tween.retarget(now, next_target, ms(250), Easing::EaseInOutCubic);
            assert_eq!(tween.value(now), before, "jump at step {step}");
        }
    }

    #[test]
    fn retarget_from_finished_tween_uses_its_target() {
        let tween = linear(0.0, 10.0);
        let retargeted = tween.retarget(ms(5000), 3.0, ms(100), Easing::Linear);
        assert_eq!(retargeted.value(ms(5000)), 10.0);
    }
}
