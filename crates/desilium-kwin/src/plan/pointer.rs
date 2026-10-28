//! Mouse settings as KWin's input device properties.
//!
//! KWin keeps mouse settings per device and exposes them as properties of
//! `org.kde.KWin.InputDevice` objects, not as a global configuration key. The plan holds what
//! every mouse should get; [`crate::apply`] sets it on each device.

use desilium_config_types::MouseSettings;

/// KWin's property names; they could not be checked against a running KWin.
pub const PROP_POINTER: &str = "pointer";
pub const PROP_TOUCHPAD: &str = "touchpad";
pub const PROP_ACCELERATION: &str = "pointerAcceleration";
pub const PROP_FLAT_PROFILE: &str = "pointerAccelerationProfileFlat";
pub const PROP_NATURAL_SCROLL: &str = "naturalScroll";
pub const PROP_LEFT_HANDED: &str = "leftHanded";
pub const PROP_SCROLL_FACTOR: &str = "scrollFactor";

/// What every mouse is set to. Whole numbers only, so plans can be compared exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerPlan {
    /// Pointer speed, `-100..=100`.
    pub speed: i16,
    /// Constant ratio instead of acceleration.
    pub flat: bool,
    pub natural_scroll: bool,
    pub left_handed: bool,
    /// Scroll speed in percent.
    pub scroll_percent: u16,
}

impl Default for PointerPlan {
    fn default() -> Self {
        Self::of(&MouseSettings::default())
    }
}

impl PointerPlan {
    /// The plan for `settings`.
    pub fn of(settings: &MouseSettings) -> Self {
        Self {
            speed: settings.pointer_speed,
            flat: settings.accel_profile.is_flat(),
            natural_scroll: settings.natural_scroll,
            left_handed: settings.left_handed,
            scroll_percent: settings.scroll_speed,
        }
    }

    /// KWin's `pointerAcceleration`: `-1.0` (slowest) to `1.0` (fastest), `0.0` neutral.
    pub fn acceleration(&self) -> f64 {
        (f64::from(self.speed) / 100.0).clamp(-1.0, 1.0)
    }

    /// KWin's `scrollFactor`: `1.0` is the normal speed.
    pub fn scroll_factor(&self) -> f64 {
        f64::from(self.scroll_percent) / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_config_types::AccelProfile;

    #[test]
    fn speed_maps_onto_kwins_acceleration_range() {
        let at = |speed| {
            PointerPlan {
                speed,
                ..PointerPlan::default()
            }
            .acceleration()
        };
        assert_eq!(at(-100), -1.0);
        assert_eq!(at(0), 0.0);
        assert_eq!(at(50), 0.5);
        assert_eq!(at(100), 1.0);
        assert_eq!(
            at(i16::MAX),
            1.0,
            "out-of-range input cannot leave KWin's range"
        );
    }

    #[test]
    fn profile_scroll_and_buttons_follow_the_settings() {
        let settings = MouseSettings {
            accel_profile: AccelProfile::Flat,
            natural_scroll: true,
            left_handed: true,
            scroll_speed: 250,
            ..MouseSettings::default()
        };
        let plan = PointerPlan::of(&settings);
        assert!(plan.flat && plan.natural_scroll && plan.left_handed);
        assert_eq!(plan.scroll_factor(), 2.5);
        assert!(!PointerPlan::default().flat);
        assert_eq!(PointerPlan::default().scroll_factor(), 1.0);
    }
}
