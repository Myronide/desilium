//! A keyed collection of tweens, such as one animated value per window.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

use crate::easing::Easing;
use crate::lerp::Lerp;
use crate::tween::Tween;

/// Runs many independent tweens, each identified by a key such as a window id.
///
/// Finished tweens keep answering [`Animator::value`] with their target until they are
/// removed with [`Animator::prune_finished`] or [`Animator::cancel`].
pub struct Animator<K: Eq + Hash + Copy, T: Lerp> {
    tweens: HashMap<K, Tween<T>>,
}

impl<K: Eq + Hash + Copy, T: Lerp> Default for Animator<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Copy, T: Lerp> Animator<K, T> {
    /// Creates an animator with no tweens.
    pub fn new() -> Self {
        Self {
            tweens: HashMap::new(),
        }
    }

    /// Installs `tween` for `key`, replacing any tween already stored under it.
    pub fn start(&mut self, key: K, tween: Tween<T>) {
        self.tweens.insert(key, tween);
    }

    /// Animates `key` toward `to`.
    ///
    /// If the key already has a tween, the new one starts from that tween's current value
    /// (see [`Tween::retarget`]). Otherwise it starts from `current_or_initial`.
    pub fn animate_to(
        &mut self,
        key: K,
        now: Duration,
        current_or_initial: T,
        to: T,
        duration: Duration,
        easing: Easing,
    ) {
        let next = match self.tweens.get(&key) {
            Some(existing) => existing.retarget(now, to, duration, easing),
            None => Tween::new(current_or_initial, to, now, duration, easing),
        };
        self.tweens.insert(key, next);
    }

    /// Returns the value of `key` at `now`, or `None` if the key has no tween.
    pub fn value(&self, key: K, now: Duration) -> Option<T> {
        self.tweens.get(&key).map(|tween| tween.value(now))
    }

    /// Returns `true` if any tween is changing its value at `now`.
    ///
    /// Redraw while this holds. The first frame where it becomes `false` lies at or after
    /// the last tween's end, so render that frame once more to show the final value.
    pub fn is_animating(&self, now: Duration) -> bool {
        self.tweens.values().any(|tween| tween.is_running(now))
    }

    /// Returns when the caller should next evaluate the animator.
    ///
    /// `Some(now)` while animating, `Some(start)` when only future tweens remain, and
    /// `None` when there is nothing left to do.
    pub fn next_wakeup(&self, now: Duration) -> Option<Duration> {
        if self.is_animating(now) {
            return Some(now);
        }
        self.tweens
            .values()
            .map(Tween::start_time)
            .filter(|start| *start > now)
            .min()
    }

    /// Removes every tween that has finished by `now`.
    pub fn prune_finished(&mut self, now: Duration) {
        self.tweens.retain(|_, tween| !tween.is_finished(now));
    }

    /// Removes the tween for `key`, if any.
    pub fn cancel(&mut self, key: K) {
        self.tweens.remove(&key);
    }

    /// Number of stored tweens, including finished ones not yet pruned.
    pub fn len(&self) -> usize {
        self.tweens.len()
    }

    /// Returns `true` if no tweens are stored.
    pub fn is_empty(&self) -> bool {
        self.tweens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn fade(start: u64) -> Tween<f32> {
        Tween::new(0.0, 1.0, ms(start), ms(100), Easing::Linear)
    }

    #[test]
    fn new_animator_is_empty_and_idle() {
        let animator: Animator<u32, f32> = Animator::new();
        assert!(animator.is_empty());
        assert_eq!(animator.len(), 0);
        assert!(!animator.is_animating(ms(0)));
        assert_eq!(animator.next_wakeup(ms(0)), None);
        assert_eq!(animator.value(1, ms(0)), None);
    }

    #[test]
    fn lifecycle_start_run_finish_prune() {
        let mut animator: Animator<u32, f32> = Animator::default();
        animator.start(7, fade(1000));
        assert_eq!(animator.len(), 1);

        assert!(!animator.is_animating(ms(1000 - 1)));
        assert!(animator.is_animating(ms(1050)));
        assert!((animator.value(7, ms(1050)).unwrap_or(-1.0) - 0.5).abs() < 1e-5);

        assert!(!animator.is_animating(ms(1100)));
        assert_eq!(animator.value(7, ms(1100)), Some(1.0));

        animator.prune_finished(ms(1100));
        assert!(animator.is_empty());
        assert_eq!(animator.value(7, ms(1100)), None);
    }

    #[test]
    fn prune_keeps_running_and_pending_tweens() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(0));
        animator.start(2, fade(500));
        animator.prune_finished(ms(50));
        assert_eq!(animator.len(), 2);
    }

    #[test]
    fn next_wakeup_reports_now_while_animating() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(0));
        assert_eq!(animator.next_wakeup(ms(30)), Some(ms(30)));
    }

    #[test]
    fn next_wakeup_reports_next_start_when_only_pending() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(400));
        animator.start(2, fade(250));
        assert_eq!(animator.next_wakeup(ms(100)), Some(ms(250)));
    }

    #[test]
    fn next_wakeup_is_none_after_everything_finished() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(0));
        assert_eq!(animator.next_wakeup(ms(500)), None);
    }

    #[test]
    fn zero_duration_tween_never_counts_as_animating() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(
            1,
            Tween::new(0.0, 1.0, ms(10), Duration::ZERO, Easing::Linear),
        );
        assert!(!animator.is_animating(ms(10)));
        assert_eq!(animator.value(1, ms(10)), Some(1.0));
        assert_eq!(animator.next_wakeup(ms(0)), Some(ms(10)));
    }

    #[test]
    fn animate_to_uses_initial_value_when_key_is_new() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.animate_to(3, ms(0), 2.0, 4.0, ms(100), Easing::Linear);
        assert_eq!(animator.value(3, ms(0)), Some(2.0));
        assert_eq!(animator.value(3, ms(100)), Some(4.0));
    }

    #[test]
    fn animate_to_retargets_running_tween_without_jump() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.animate_to(3, ms(0), 0.0, 10.0, ms(1000), Easing::Linear);
        let before = animator.value(3, ms(400)).unwrap_or(f32::NAN);

        // The caller's own notion of "current" is ignored once the key is running.
        animator.animate_to(3, ms(400), -999.0, 0.0, ms(200), Easing::Linear);
        assert_eq!(animator.value(3, ms(400)), Some(before));
        assert_eq!(animator.value(3, ms(600)), Some(0.0));
    }

    #[test]
    fn animate_to_on_finished_key_starts_from_its_target() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.animate_to(1, ms(0), 0.0, 5.0, ms(100), Easing::Linear);
        animator.animate_to(1, ms(500), 0.0, 9.0, ms(100), Easing::Linear);
        assert_eq!(animator.value(1, ms(500)), Some(5.0));
        assert_eq!(animator.value(1, ms(600)), Some(9.0));
    }

    #[test]
    fn cancel_removes_one_key_only() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(0));
        animator.start(2, fade(0));
        animator.cancel(1);
        assert_eq!(animator.len(), 1);
        assert_eq!(animator.value(1, ms(50)), None);
        assert!(animator.value(2, ms(50)).is_some());
        animator.cancel(99);
        assert_eq!(animator.len(), 1);
    }

    #[test]
    fn start_replaces_existing_tween() {
        let mut animator: Animator<u32, f32> = Animator::new();
        animator.start(1, fade(0));
        animator.start(1, Tween::new(5.0, 6.0, ms(0), ms(100), Easing::Linear));
        assert_eq!(animator.len(), 1);
        assert_eq!(animator.value(1, ms(0)), Some(5.0));
    }

    #[test]
    fn works_with_tuple_values_and_array_keys() {
        let mut animator: Animator<(u8, u8), (f32, f32)> = Animator::new();
        animator.animate_to(
            (1, 2),
            ms(0),
            (0.0, 0.0),
            (10.0, 20.0),
            ms(100),
            Easing::Linear,
        );
        assert_eq!(animator.value((1, 2), ms(50)), Some((5.0, 10.0)));
    }
}
