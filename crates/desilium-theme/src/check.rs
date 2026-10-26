//! Building blocks of theme validation: range repair and contrast repair.

use std::fmt;

use crate::Color;

/// WCAG AA for normal text.
pub const MIN_TEXT_CONTRAST: f32 = 4.5;
/// WCAG AA for large or de-emphasised text.
pub const MIN_DISABLED_CONTRAST: f32 = 3.0;

const CONTRAST_STEPS: u8 = 25;
const CONTRAST_STEP_SIZE: f32 = 0.04;

/// One value that [`crate::Theme::validate`] had to change.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeIssue {
    /// Dotted path of the field, e.g. `window.corner_radius`.
    pub field: String,
    /// Human-readable description including old and new value.
    pub message: String,
}

impl fmt::Display for ThemeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Inclusive numeric range plus the value used when the input is NaN/infinite.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bounds {
    pub min: f32,
    pub max: f32,
    pub fallback: f32,
}

impl Bounds {
    pub(crate) const fn new(min: f32, max: f32, fallback: f32) -> Bounds {
        Bounds { min, max, fallback }
    }
}

/// Collects issues while repairing values in place.
#[derive(Debug, Default)]
pub(crate) struct Checker {
    pub issues: Vec<ThemeIssue>,
}

impl Checker {
    pub(crate) fn report(&mut self, field: &str, message: String) {
        self.issues.push(ThemeIssue {
            field: field.to_owned(),
            message,
        });
    }

    /// Clamps `value` into `bounds`; non-finite values take the fallback.
    pub(crate) fn float(&mut self, field: &str, value: &mut f32, bounds: Bounds) {
        let repaired = match value.is_finite() {
            true => value.clamp(bounds.min, bounds.max),
            false => bounds.fallback,
        };
        if repaired == *value {
            return;
        }
        self.report(
            field,
            format!(
                "{value} is outside {}..={}; using {repaired}",
                bounds.min, bounds.max
            ),
        );
        *value = repaired;
    }

    /// Clamps an integer value into `min..=max`.
    pub(crate) fn int(&mut self, field: &str, value: &mut u32, range: (u32, u32)) {
        let repaired = (*value).clamp(range.0, range.1);
        if repaired == *value {
            return;
        }
        self.report(
            field,
            format!(
                "{value} is outside {}..={}; using {repaired}",
                range.0, range.1
            ),
        );
        *value = repaired;
    }

    /// Forces channels into range and optionally demands full opacity.
    pub(crate) fn color(&mut self, field: &str, value: &mut Color, opaque: bool) {
        let mut repaired = value.clamped();
        if opaque {
            repaired.a = 1.0;
        }
        if repaired == *value {
            return;
        }
        self.report(
            field,
            format!(
                "{value} is not a valid{} colour; using {repaired}",
                if opaque { " opaque" } else { "" }
            ),
        );
        *value = repaired;
    }

    /// Moves `fg` toward white or black until it reaches `min` against every `bgs`.
    pub(crate) fn contrast(&mut self, field: &str, fg: &mut Color, bgs: &[Color], min: f32) {
        let worst = |candidate: Color| {
            bgs.iter()
                .map(|bg| candidate.contrast_ratio(*bg))
                .fold(f32::MAX, f32::min)
        };
        let before = worst(*fg);
        if before >= min {
            return;
        }
        let target = match worst(Color::WHITE) >= worst(Color::BLACK) {
            true => Color::WHITE,
            false => Color::BLACK,
        };
        let original = *fg;
        let repaired = (1..=CONTRAST_STEPS)
            .map(|step| original.mix(target, f32::from(step) * CONTRAST_STEP_SIZE))
            .find(|candidate| worst(*candidate) >= min)
            .unwrap_or(target);
        self.report(
            field,
            format!("contrast {before:.2}:1 is below {min}:1; {original} became {repaired}"),
        );
        *fg = repaired;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Bounds = Bounds::new(0.0, 10.0, 4.0);

    #[test]
    fn float_in_range_is_untouched() {
        let mut checker = Checker::default();
        let mut value = 7.0;
        checker.float("x", &mut value, BOUNDS);
        assert!(checker.issues.is_empty());
        assert_eq!(value, 7.0);
    }

    #[test]
    fn float_out_of_range_and_nan_are_repaired() {
        let mut checker = Checker::default();
        let (mut low, mut high, mut nan) = (-3.0, 99.0, f32::NAN);
        checker.float("a", &mut low, BOUNDS);
        checker.float("b", &mut high, BOUNDS);
        checker.float("c", &mut nan, BOUNDS);
        assert_eq!((low, high, nan), (0.0, 10.0, 4.0));
        assert_eq!(checker.issues.len(), 3);
    }

    #[test]
    fn contrast_repair_reaches_the_target() {
        let mut checker = Checker::default();
        let background = Color::rgb(0x101010);
        let mut text = Color::rgb(0x202020);
        checker.contrast("t", &mut text, &[background], MIN_TEXT_CONTRAST);
        assert!(text.contrast_ratio(background) >= MIN_TEXT_CONTRAST);
        assert_eq!(checker.issues.len(), 1);
    }

    #[test]
    fn contrast_repair_handles_conflicting_backgrounds() {
        let mut checker = Checker::default();
        let backgrounds = [Color::BLACK, Color::WHITE];
        let mut text = Color::rgb(0x808080);
        checker.contrast("t", &mut text, &backgrounds, MIN_TEXT_CONTRAST);
        assert_eq!(
            checker.issues.len(),
            1,
            "unsatisfiable input must still terminate"
        );
    }

    #[test]
    fn colour_repair_forces_opacity_when_asked() {
        let mut checker = Checker::default();
        let mut color = Color::WHITE.with_alpha(0.4);
        checker.color("c", &mut color, true);
        assert_eq!(color.a, 1.0);
        checker.color("c", &mut color, true);
        assert_eq!(checker.issues.len(), 1);
    }
}
