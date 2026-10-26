//! Linear interpolation for animatable value types.

/// A value that can be interpolated between two endpoints.
pub trait Lerp: Copy {
    /// Returns the point at fraction `t` between `self` (0) and `to` (1).
    ///
    /// `t` is not clamped here; callers such as [`crate::Tween`] clamp it first.
    fn lerp(self, to: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, to: Self, t: f32) -> Self {
        // Written as a weighted sum so both endpoints are reproduced exactly.
        (1.0 - t) * self + t * to
    }
}

impl Lerp for f64 {
    fn lerp(self, to: Self, t: f32) -> Self {
        let t = f64::from(t);
        (1.0 - t) * self + t * to
    }
}

impl Lerp for (f32, f32) {
    fn lerp(self, to: Self, t: f32) -> Self {
        (self.0.lerp(to.0, t), self.1.lerp(to.1, t))
    }
}

impl Lerp for [f32; 4] {
    fn lerp(self, to: Self, t: f32) -> Self {
        let mut out = self;
        for (value, target) in out.iter_mut().zip(to) {
            *value = (*value).lerp(target, t);
        }
        out
    }
}

impl Lerp for (i32, i32) {
    fn lerp(self, to: Self, t: f32) -> Self {
        (lerp_rounded(self.0, to.0, t), lerp_rounded(self.1, to.1, t))
    }
}

/// Interpolates in `f64` (exact for every `i32`) and rounds half away from zero.
fn lerp_rounded(from: i32, to: i32, t: f32) -> i32 {
    let t = f64::from(t);
    let value = (1.0 - t) * f64::from(from) + t * f64::from(to);
    // `as` saturates at the i32 bounds and maps NaN to 0, which is the safe fallback.
    value.round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_hits_both_endpoints_exactly() {
        assert_eq!(0.1_f32.lerp(0.7, 0.0), 0.1);
        assert_eq!(0.1_f32.lerp(0.7, 1.0), 0.7);
    }

    #[test]
    fn f32_midpoint() {
        assert!((0.0_f32.lerp(10.0, 0.5) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn f64_interpolates() {
        assert!((2.0_f64.lerp(4.0, 0.25) - 2.5).abs() < 1e-12);
        assert_eq!(2.0_f64.lerp(4.0, 1.0), 4.0);
    }

    #[test]
    fn tuple_and_array_interpolate_componentwise() {
        assert_eq!((0.0_f32, 10.0_f32).lerp((2.0, 20.0), 0.5), (1.0, 15.0));
        assert_eq!(
            [0.0_f32, 1.0, 2.0, 3.0].lerp([4.0, 5.0, 6.0, 7.0], 0.5),
            [2.0, 3.0, 4.0, 5.0]
        );
    }

    #[test]
    fn integer_pairs_round_to_nearest() {
        assert_eq!((0_i32, 10_i32).lerp((10, 20), 0.5), (5, 15));
        assert_eq!((0_i32, 10_i32).lerp((10, 20), 0.26), (3, 13));
        assert_eq!((0_i32, 0_i32).lerp((10, 20), 0.0), (0, 0));
        assert_eq!((0_i32, 0_i32).lerp((10, 20), 1.0), (10, 20));
    }

    #[test]
    fn integer_pairs_do_not_overflow_at_extremes() {
        let (low, high) = (i32::MIN, i32::MAX);
        assert_eq!((low, low).lerp((high, high), 0.0), (low, low));
        assert_eq!((low, low).lerp((high, high), 1.0), (high, high));
    }
}
