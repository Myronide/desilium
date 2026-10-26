//! Easing curves that reshape linear progress into motion.

/// Base of the exponential falloff: `2^(-EXPO_STEEPNESS * t)`.
const EXPO_STEEPNESS: f32 = 10.0;
/// Value of `2^-10`, the curve's value at the start, used to normalise the expo curves.
const EXPO_FLOOR: f32 = 1.0 / 1024.0;
/// Value of `2^-11`, the in-out expo curve's value at its start.
const EXPO_IN_OUT_FLOOR: f32 = 1.0 / 2048.0;
/// Overshoot constant of the standard ease-out-back curve.
const BACK_OVERSHOOT: f32 = 1.70158;
/// Bisection steps when inverting the Bezier x(s) curve; far below f32 resolution.
const BEZIER_ITERATIONS: u32 = 48;

/// A curve that maps linear progress `t` in `0..=1` onto eased progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    /// No shaping: `apply(t) == t`.
    Linear,
    /// Quadratic deceleration.
    EaseOutQuad,
    /// Cubic deceleration.
    EaseOutCubic,
    /// Cubic acceleration into cubic deceleration.
    EaseInOutCubic,
    /// Fast start with a long, smooth settle.
    EaseOutExpo,
    /// Exponential acceleration into exponential deceleration.
    EaseInOutExpo,
    /// Overshoots the target slightly before settling.
    EaseOutBack,
    /// CSS-style cubic Bezier `(x1, y1, x2, y2)`; `x1` and `x2` lie in `0..=1`.
    CubicBezier(f32, f32, f32, f32),
}

/// Clamps progress to `0..=1`; NaN maps to 0 so bad input never causes a jump.
pub(crate) fn clamp_unit(t: f32) -> f32 {
    if t.is_nan() {
        return 0.0;
    }
    t.clamp(0.0, 1.0)
}

impl Easing {
    /// Maps linear progress `t` (clamped to `0..=1`) to eased progress.
    ///
    /// Returns exactly `0.0` at the start and exactly `1.0` at the end for every variant.
    pub fn apply(self, t: f32) -> f32 {
        let t = clamp_unit(t);
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        match self {
            Easing::Linear => t,
            Easing::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOutCubic => ease_in_out_cubic(t),
            Easing::EaseOutExpo => ease_out_expo(t),
            Easing::EaseInOutExpo => ease_in_out_expo(t),
            Easing::EaseOutBack => ease_out_back(t),
            Easing::CubicBezier(x1, y1, x2, y2) => cubic_bezier(t, x1, y1, x2, y2),
        }
    }

    /// Parses a CSS-like name such as `"ease-out-expo"` or `"cubic-bezier(0.2,0.8,0.2,1)"`.
    ///
    /// Matching ignores case and surrounding whitespace. Returns `None` for unknown names,
    /// malformed or non-finite Bezier arguments, and Bezier `x` values outside `0..=1`.
    pub fn from_name(name: &str) -> Option<Easing> {
        let name = name.trim().to_ascii_lowercase();
        if let Some(args) = name
            .strip_prefix("cubic-bezier(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            return parse_cubic_bezier(args);
        }
        match name.as_str() {
            "linear" => Some(Easing::Linear),
            "ease" => Some(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)),
            "ease-out-quad" => Some(Easing::EaseOutQuad),
            "ease-out-cubic" => Some(Easing::EaseOutCubic),
            "ease-in-out-cubic" => Some(Easing::EaseInOutCubic),
            "ease-out-expo" => Some(Easing::EaseOutExpo),
            "ease-in-out-expo" => Some(Easing::EaseInOutExpo),
            "ease-out-back" => Some(Easing::EaseOutBack),
            _ => None,
        }
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn ease_out_expo(t: f32) -> f32 {
    (1.0 - 2f32.powf(-EXPO_STEEPNESS * t)) / (1.0 - EXPO_FLOOR)
}

fn ease_in_out_expo(t: f32) -> f32 {
    let raw = if t < 0.5 {
        2f32.powf(2.0 * EXPO_STEEPNESS * t - EXPO_STEEPNESS) / 2.0
    } else {
        1.0 - 2f32.powf(-2.0 * EXPO_STEEPNESS * t + EXPO_STEEPNESS) / 2.0
    };
    (raw - EXPO_IN_OUT_FLOOR) / (1.0 - 2.0 * EXPO_IN_OUT_FLOOR)
}

fn ease_out_back(t: f32) -> f32 {
    let shifted = t - 1.0;
    let cubic_coefficient = BACK_OVERSHOOT + 1.0;
    1.0 + cubic_coefficient * shifted.powi(3) + BACK_OVERSHOOT * shifted * shifted
}

/// Solves x(s) = t by bisection, then evaluates y(s).
///
/// Bisection is used instead of Newton's method because x(s) is monotonic for
/// `x1, x2` in `0..=1`, so it always converges, even for flat or steep curves.
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x1 = clamp_unit(x1);
    let x2 = clamp_unit(x2);
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..BEZIER_ITERATIONS {
        let mid = 0.5 * (low + high);
        if bezier_component(mid, x1, x2) < t {
            low = mid;
        } else {
            high = mid;
        }
    }
    bezier_component(0.5 * (low + high), y1, y2)
}

/// One coordinate of a cubic Bezier with endpoints 0 and 1 and control values `p1`, `p2`.
fn bezier_component(s: f32, p1: f32, p2: f32) -> f32 {
    let inv = 1.0 - s;
    3.0 * inv * inv * s * p1 + 3.0 * inv * s * s * p2 + s * s * s
}

fn parse_cubic_bezier(args: &str) -> Option<Easing> {
    let mut values = [0.0_f32; 4];
    let mut parts = args.split(',');
    for slot in &mut values {
        let value: f32 = parts.next()?.trim().parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    if parts.next().is_some() {
        return None;
    }
    let [x1, y1, x2, y2] = values;
    if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
        return None;
    }
    Some(Easing::CubicBezier(x1, y1, x2, y2))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 8] = [
        Easing::Linear,
        Easing::EaseOutQuad,
        Easing::EaseOutCubic,
        Easing::EaseInOutCubic,
        Easing::EaseOutExpo,
        Easing::EaseInOutExpo,
        Easing::EaseOutBack,
        Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
    ];

    const MONOTONIC: [Easing; 7] = [
        Easing::Linear,
        Easing::EaseOutQuad,
        Easing::EaseOutCubic,
        Easing::EaseInOutCubic,
        Easing::EaseOutExpo,
        Easing::EaseInOutExpo,
        Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
    ];

    const SAMPLES: usize = 200;

    fn sample(i: usize) -> f32 {
        i as f32 / SAMPLES as f32
    }

    #[test]
    fn every_curve_hits_endpoints_exactly() {
        for easing in ALL {
            assert_eq!(easing.apply(0.0), 0.0, "{easing:?} start");
            assert_eq!(easing.apply(1.0), 1.0, "{easing:?} end");
        }
    }

    #[test]
    fn out_of_range_and_nan_input_is_clamped() {
        for easing in ALL {
            assert_eq!(easing.apply(-5.0), 0.0, "{easing:?} below range");
            assert_eq!(easing.apply(7.0), 1.0, "{easing:?} above range");
            assert_eq!(easing.apply(f32::NAN), 0.0, "{easing:?} NaN");
            assert_eq!(easing.apply(f32::INFINITY), 1.0, "{easing:?} inf");
            assert_eq!(easing.apply(f32::NEG_INFINITY), 0.0, "{easing:?} -inf");
        }
    }

    #[test]
    fn monotonic_curves_never_decrease() {
        for easing in MONOTONIC {
            let mut previous = easing.apply(0.0);
            for i in 1..=SAMPLES {
                let current = easing.apply(sample(i));
                assert!(current >= previous - 1e-6, "{easing:?} decreased at {i}");
                previous = current;
            }
        }
    }

    #[test]
    fn linear_is_identity() {
        for i in 0..=SAMPLES {
            let t = sample(i);
            assert!((Easing::Linear.apply(t) - t).abs() < 1e-6);
        }
    }

    #[test]
    fn ease_out_back_overshoots_then_settles() {
        let peak = (0..=SAMPLES)
            .map(|i| Easing::EaseOutBack.apply(sample(i)))
            .fold(f32::MIN, f32::max);
        assert!(peak > 1.05, "expected overshoot, peak was {peak}");
        assert!(Easing::EaseOutBack.apply(0.9) < peak);
    }

    #[test]
    fn symmetric_in_out_curves_pass_through_half() {
        assert!((Easing::EaseInOutCubic.apply(0.5) - 0.5).abs() < 1e-6);
        assert!((Easing::EaseInOutExpo.apply(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn expo_out_is_front_loaded() {
        assert!(Easing::EaseOutExpo.apply(0.25) > 0.8);
        assert!(Easing::EaseOutQuad.apply(0.5) > 0.7);
    }

    #[test]
    fn css_ease_matches_reference_value() {
        let ease = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
        assert!(
            (ease.apply(0.5) - 0.8024).abs() < 1e-3,
            "{}",
            ease.apply(0.5)
        );
    }

    #[test]
    fn cubic_bezier_linear_control_points_are_identity() {
        let linear = Easing::CubicBezier(0.0, 0.0, 1.0, 1.0);
        for i in 0..=SAMPLES {
            let t = sample(i);
            assert!((linear.apply(t) - t).abs() < 1e-3, "at {t}");
        }
    }

    #[test]
    fn cubic_bezier_matches_css_ease_in_out() {
        // CSS `ease-in-out` is cubic-bezier(0.42, 0, 0.58, 1); its midpoint is exactly 0.5.
        let ease_in_out = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);
        assert!((ease_in_out.apply(0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn from_name_parses_known_names() {
        assert_eq!(Easing::from_name("linear"), Some(Easing::Linear));
        assert_eq!(
            Easing::from_name("ease-out-expo"),
            Some(Easing::EaseOutExpo)
        );
        assert_eq!(
            Easing::from_name("ease-in-out-expo"),
            Some(Easing::EaseInOutExpo)
        );
        assert_eq!(
            Easing::from_name("ease-out-back"),
            Some(Easing::EaseOutBack)
        );
        assert_eq!(
            Easing::from_name("ease"),
            Some(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0))
        );
    }

    #[test]
    fn from_name_ignores_case_and_whitespace() {
        assert_eq!(
            Easing::from_name("  EASE-OUT-CUBIC "),
            Some(Easing::EaseOutCubic)
        );
    }

    #[test]
    fn from_name_parses_cubic_bezier() {
        assert_eq!(
            Easing::from_name("cubic-bezier(0.2,0.8,0.2,1)"),
            Some(Easing::CubicBezier(0.2, 0.8, 0.2, 1.0))
        );
        assert_eq!(
            Easing::from_name("cubic-bezier( 0.2 , 0.8, 0.2, 1 )"),
            Some(Easing::CubicBezier(0.2, 0.8, 0.2, 1.0))
        );
        assert_eq!(
            Easing::from_name("cubic-bezier(0.3,-0.5,0.7,1.5)"),
            Some(Easing::CubicBezier(0.3, -0.5, 0.7, 1.5))
        );
    }

    #[test]
    fn from_name_rejects_invalid_input() {
        let invalid = [
            "",
            "bogus",
            "ease-out",
            "cubic-bezier(1,2,3)",
            "cubic-bezier(1,2,3,4,5)",
            "cubic-bezier(a,b,c,d)",
            "cubic-bezier(1.5,0,0,1)",
            "cubic-bezier(-0.1,0,0,1)",
            "cubic-bezier(nan,0,0,1)",
            "cubic-bezier(inf,0,0,1)",
            "cubic-bezier(0,0,1,1",
            "cubic-bezier(,,,)",
        ];
        for name in invalid {
            assert_eq!(Easing::from_name(name), None, "accepted {name:?}");
        }
    }
}
