//! Crossfade timing for wallpaper changes.

use desilium_anim::{Easing, Tween};
use std::time::Duration;

/// Length of the gradient-to-wallpaper (and wallpaper-to-wallpaper) crossfade.
pub const FADE_DURATION: Duration = Duration::from_millis(400);

/// Eased 0 -> 1 progress of one crossfade; time is passed in, never read.
#[derive(Debug, Clone, Copy)]
pub struct Fade(Tween<f32>);

impl Fade {
    /// Starts at `now`; with `animate` false the fade is already complete.
    pub fn start(now: Duration, animate: bool) -> Self {
        let duration = if animate {
            FADE_DURATION
        } else {
            Duration::ZERO
        };
        Self(Tween::new(0.0, 1.0, now, duration, Easing::EaseInOutCubic))
    }

    /// Opacity of the incoming layer at `now`, in `0.0..=1.0`.
    pub fn progress(&self, now: Duration) -> f32 {
        self.0.value(now).clamp(0.0, 1.0)
    }

    /// Whether the fade has reached its end.
    pub fn is_finished(&self, now: Duration) -> bool {
        self.0.is_finished(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: Duration = Duration::from_secs(10);

    #[test]
    fn progress_runs_from_zero_to_one() {
        let fade = Fade::start(START, true);
        assert_eq!(fade.progress(START), 0.0);
        let middle = fade.progress(START + FADE_DURATION / 2);
        assert!((middle - 0.5).abs() < 0.01, "{middle}");
        assert_eq!(fade.progress(START + FADE_DURATION), 1.0);
        assert!(!fade.is_finished(START + FADE_DURATION / 2));
        assert!(fade.is_finished(START + FADE_DURATION));
    }

    #[test]
    fn disabled_animation_finishes_immediately() {
        let fade = Fade::start(START, false);
        assert!(fade.is_finished(START));
        assert_eq!(fade.progress(START), 1.0);
    }
}
