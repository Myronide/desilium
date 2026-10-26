//! Ready-made open and close transitions for windows.

use crate::easing::{Easing, clamp_unit};
use crate::lerp::Lerp;

/// Scale a window grows from when it opens (and shrinks to when it closes).
pub const WINDOW_SCALE_FROM: f32 = 0.94;

/// Returns `(scale, alpha)` for a window opening at progress `t` in `0..=1`.
///
/// Scale grows from [`WINDOW_SCALE_FROM`] to 1 and alpha fades from 0 to 1, both on the
/// ease-out-expo curve.
pub fn open_scale_alpha(t: f32) -> (f32, f32) {
    let eased = Easing::EaseOutExpo.apply(t);
    (WINDOW_SCALE_FROM.lerp(1.0, eased), eased)
}

/// Returns `(scale, alpha)` for a window closing at progress `t` in `0..=1`.
///
/// This is the open curve played backwards: fully visible at `t = 0`, and shrunk and
/// transparent at `t = 1`.
pub fn close_scale_alpha(t: f32) -> (f32, f32) {
    open_scale_alpha(1.0 - clamp_unit(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-6;

    #[test]
    fn open_starts_small_and_transparent() {
        let (scale, alpha) = open_scale_alpha(0.0);
        assert!((scale - WINDOW_SCALE_FROM).abs() < EPSILON);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn open_ends_at_full_size_and_opacity() {
        let (scale, alpha) = open_scale_alpha(1.0);
        assert_eq!(scale, 1.0);
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn open_is_monotonic_in_scale_and_alpha() {
        let mut previous = (f32::MIN, f32::MIN);
        for i in 0..=100 {
            let current = open_scale_alpha(i as f32 / 100.0);
            assert!(current.0 >= previous.0 - EPSILON);
            assert!(current.1 >= previous.1 - EPSILON);
            previous = current;
        }
    }

    #[test]
    fn close_is_open_played_backwards() {
        let (scale, alpha) = close_scale_alpha(0.0);
        assert_eq!((scale, alpha), (1.0, 1.0));
        let (scale, alpha) = close_scale_alpha(1.0);
        assert!((scale - WINDOW_SCALE_FROM).abs() < EPSILON);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn invalid_progress_is_clamped() {
        assert_eq!(open_scale_alpha(f32::NAN), open_scale_alpha(0.0));
        assert_eq!(open_scale_alpha(-3.0), open_scale_alpha(0.0));
        assert_eq!(open_scale_alpha(3.0), open_scale_alpha(1.0));
        assert_eq!(close_scale_alpha(f32::NAN), close_scale_alpha(0.0));
    }
}
